use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use reqwest::header::{CONTENT_RANGE, HeaderMap, IF_RANGE, RANGE};
use reqwest::{Client, Response, StatusCode, Url};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt, BufWriter};
use tokio::task::JoinSet;

use crate::control::{CancelToken, ConnectionRange, Progress, RangeSource};
use crate::error::EngineError;
use crate::http::{ContentRange, client, get, parse_content_range, parse_url};
use crate::limits::{Adaptive, ConnectionBudget, MIN_PER_CONNECTION, Permit, SpeedLimit};
use crate::probe::{self, ProbeInfo, Validator, probe};
use crate::rangemap::{self, RangeMap};
use crate::ranges::{Range, Table, gaps};
use crate::retry::{self, RetryPolicy, retryable};

/// Connections per file unless the caller asks otherwise (specs/engine.md).
pub const DEFAULT_CONNECTIONS: usize = 16;
/// Never more than this many connections for one file (specs/engine.md, "Being a good client").
pub const MAX_CONNECTIONS: usize = 32;
/// Each connection buffers about this much before writing, so the disk is not hit per packet.
const WRITE_BUFFER: usize = 1 << 20;

#[derive(Clone, Debug)]
pub struct DownloadOptions {
    /// 1 to 32. Fewer are used when fewer bytes are left, or the server has no range support.
    pub connections: usize,
    /// Poll `progress.snapshot()` from anywhere while the download runs.
    pub progress: Progress,
    /// Call `cancel.cancel()` to stop (pause) every connection; the download returns
    /// `Cancelled` and keeps what it has for a later resume.
    pub cancel: CancelToken,
    /// Per-range retry and backoff.
    pub retry: RetryPolicy,
    /// How often the range map is saved while the download runs (it is also saved when the
    /// download stops). Bytes fetched since the last save are fetched again after a crash.
    pub checkpoint_interval: Duration,
    /// Adapt the number of connections (up to `connections`) to what the server and the
    /// network handle. `None`: always `connections` (fewer only when fewer bytes are left).
    pub adaptive: Option<Adaptive>,
    /// Connections shared with other downloads: each connection holds one while it runs.
    pub budget: Option<ConnectionBudget>,
    /// A speed limit shared with other downloads.
    pub speed_limit: Option<SpeedLimit>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            connections: DEFAULT_CONNECTIONS,
            progress: Progress::new(),
            cancel: CancelToken::new(),
            retry: RetryPolicy::default(),
            checkpoint_interval: Duration::from_secs(2),
            adaptive: None,
            budget: None,
            speed_limit: None,
        }
    }
}

/// One byte range of the file, inclusive at both ends, and how much of it arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub start: u64,
    pub end: u64,
    pub received: u64,
}

impl Segment {
    pub fn len(&self) -> u64 {
        self.end - self.start + 1
    }

    pub fn is_empty(&self) -> bool {
        self.end < self.start
    }
}

impl From<Range> for Segment {
    fn from(r: Range) -> Self {
        Self { start: r.start, end: r.end - 1, received: r.len() }
    }
}

#[derive(Clone, Debug)]
pub struct DownloadOutcome {
    pub probe: ProbeInfo,
    /// Bytes in the file; equals the file size.
    pub bytes: u64,
    /// Bytes kept from an earlier run (0 when nothing was resumed).
    pub resumed: u64,
    /// The ranges fetched in this run, in file order. With work stealing there can be more
    /// ranges than connections. Resumed bytes are not listed.
    pub segments: Vec<Segment>,
}

/// What a range map on disk says about an unfinished download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedState {
    /// The URL the download was started with.
    pub url: String,
    pub size: u64,
    /// The byte ranges already in the file.
    pub done: Vec<Segment>,
}

impl SavedState {
    pub fn done_bytes(&self) -> u64 {
        self.done.iter().map(Segment::len).sum()
    }
}

/// The unfinished-download state saved next to `destination`, if there is a valid one.
/// `download` with the same URL and destination resumes from it.
pub async fn saved_state(destination: &Path) -> Option<SavedState> {
    let map = rangemap::load(destination).await?;
    Some(SavedState { url: map.url, size: map.size, done: map.done.into_iter().map(Segment::from).collect() })
}

/// Deletes a partial download: the file and its range map. Missing files are not an error.
pub async fn discard(destination: &Path) -> std::io::Result<()> {
    rangemap::remove(destination).await?;
    match tokio::fs::remove_file(destination).await {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Splits `size` bytes into at most `n` ranges of near-equal length.
pub(crate) fn split(size: u64, n: usize) -> Vec<Segment> {
    let n = (n as u64).clamp(1, size.max(1));
    let base = size / n;
    let extra = size % n;
    let mut start = 0;
    (0..n)
        .map(|i| {
            let len = base + u64::from(i < extra);
            let seg = Segment { start, end: start + len - 1, received: 0 };
            start += len;
            seg
        })
        .collect()
}

/// Probes `url`, then downloads it to `destination`.
///
/// With range support and a known size, the file is pre-allocated and fetched over up to
/// `options.connections` connections that share the work (an idle connection takes half of
/// the range expected to finish last). Each range is retried on its own. A range map next
/// to the file (`<destination>.ksdm`) records the finished bytes; if one from an earlier
/// run matches this URL, size and validator, only the missing bytes are fetched, each
/// request carrying `If-Range`. If the file changed on the server the download starts
/// over. On cancel, or when retries run out, the partial file and map are kept for a
/// resume (call [`discard`] to delete them); on other failures both are removed.
///
/// Without range support (or with an unknown size) one connection writes the file in
/// order and a retry starts from the beginning; such a download cannot be resumed.
pub async fn download(url: &str, destination: &Path, options: DownloadOptions) -> Result<DownloadOutcome, EngineError> {
    if !(1..=MAX_CONNECTIONS).contains(&options.connections) {
        return Err(EngineError::InvalidOptions(format!(
            "connections must be 1 to {MAX_CONNECTIONS}, got {}",
            options.connections
        )));
    }
    if options.retry.max_attempts == 0 {
        return Err(EngineError::InvalidOptions("retry.max_attempts must be at least 1".into()));
    }
    // A file that changes while we fetch it is restarted once; a second change is an error.
    let mut may_resume = true;
    loop {
        let info = tokio::select! {
            biased;
            _ = options.cancel.cancelled() => return Err(EngineError::Cancelled),
            r = probe(url) => r?,
        };
        let result = match info.size {
            Some(size) if info.ranges && size > 0 => segmented(url, info, size, destination, &options, may_resume).await,
            _ => single(info, destination, &options).await,
        };
        match result {
            Err(EngineError::FileChanged) if may_resume => may_resume = false,
            r => return r,
        }
    }
}

/// Publishes the connections' ranges in `progress` while alive, and stops when dropped,
/// so snapshots taken after the download stopped show no connection at work.
struct ShowRanges<'a>(&'a Progress);

impl<'a> ShowRanges<'a> {
    fn new(progress: &'a Progress, source: RangeSource) -> Self {
        progress.set_ranges(Some(source));
        Self(progress)
    }
}

impl Drop for ShowRanges<'_> {
    fn drop(&mut self) {
        self.0.set_ranges(None);
    }
}

/// Errors after which the partial file is worth keeping for a resume. `Internal` is kept
/// too: it is our bug, not bad data, and every byte in the map was verified when written.
fn keeps_partial(e: &EngineError) -> bool {
    matches!(e, EngineError::Cancelled | EngineError::Io(_) | EngineError::Internal(_)) || retryable(e)
}

async fn segmented(
    source: &str,
    info: ProbeInfo,
    size: u64,
    destination: &Path,
    options: &DownloadOptions,
    may_resume: bool,
) -> Result<DownloadOutcome, EngineError> {
    let url = parse_url(&info.final_url)?;
    // A strong validator is needed to prove the bytes on disk belong to this file.
    let validator = info.validator.clone().filter(|v| v.if_range().is_some());
    let on_disk = tokio::fs::metadata(destination).await.ok().map(|m| m.len());
    let saved = match (may_resume, &validator) {
        (true, Some(v)) => rangemap::load(destination)
            .await
            .filter(|m| m.url == source && m.size == size && m.validator.as_ref() == Some(v) && on_disk == Some(size)),
        _ => None,
    };
    let resumed = saved.as_ref().map_or(0, RangeMap::done_bytes);
    let done = saved.map(|m| m.done).unwrap_or_default();
    if done.is_empty() {
        // The old map goes first: a crash after truncating must never leave a map that
        // describes bytes the zero-filled file no longer has.
        rangemap::remove(destination).await?;
        let file = File::create(destination).await?;
        file.set_len(size).await?; // pre-allocate; ranges write straight into place
    }

    let todo = gaps(&done, size);
    let remaining: u64 = todo.iter().map(|r| r.len()).sum();
    let host = url.host_str().unwrap_or_default().to_owned();
    let (slots, start) = plan(options, remaining, &host);
    let pending = if done.is_empty() {
        split(size, start).into_iter().map(|s| Range { start: s.start, end: s.end + 1 }).collect()
    } else {
        todo
    };
    options.progress.reset(Some(size), slots);
    options.progress.set_base(resumed);

    let ctx = Arc::new(Ctx {
        source: source.to_owned(),
        url,
        total: size,
        if_range: Mutex::new(validator.as_ref().and_then(Validator::if_range).map(str::to_owned)),
        validator,
        destination: destination.to_path_buf(),
        table: Mutex::new(Table::with_slots(slots, pending, done)),
        counters: options.progress.counters(),
        retry: options.retry.clone(),
        flush_every: options.checkpoint_interval,
        adaptive: options.adaptive.clone(),
        budget: options.budget.clone(),
        speed_limit: options.speed_limit.clone(),
        trouble: AtomicU64::new(0),
    });
    // Snapshots read the table only while this download runs (also if the caller drops it).
    let weak = Arc::downgrade(&ctx);
    let _ranges = ShowRanges::new(
        &options.progress,
        RangeSource(Arc::new(move || weak.upgrade().map(|c| c.table().assignments()).unwrap_or_default())),
    );

    let mut result = match ctx.checkpoint().await {
        Ok(()) => run_workers(&ctx, &host, start, &options.cancel, options.checkpoint_interval).await,
        Err(e) => Err(e.into()),
    };
    if result.is_ok() && ctx.table().written() != [Range { start: 0, end: size }] {
        result = Err(EngineError::Internal("ranges finished but the file is not complete".into()));
    }
    let result = match result {
        Ok(()) => check_size(destination, size, size).await,
        Err(e) => Err(e),
    };
    match result {
        Ok(()) => {
            rangemap::remove(destination).await?;
            options.progress.set_total(size);
            let segments = ctx.table().fetched();
            Ok(DownloadOutcome { probe: info, bytes: size, resumed, segments })
        }
        Err(e) => {
            let e = if options.cancel.is_cancelled() { EngineError::Cancelled } else { e };
            if keeps_partial(&e) && ctx.validator.is_some() {
                // Best effort: the last periodic checkpoint is still on disk if this fails.
                let _ = ctx.checkpoint().await;
            } else {
                let _ = discard(destination).await;
            }
            Err(e)
        }
    }
}

async fn check_size(destination: &Path, expected: u64, written: u64) -> Result<(), EngineError> {
    let on_disk = tokio::fs::metadata(destination).await?.len();
    if written != expected || on_disk != expected {
        return Err(EngineError::SizeMismatch { expected, written: written.min(on_disk) });
    }
    Ok(())
}

/// How many connections a download may have (`slots`) and starts with. Without
/// `adaptive`: `connections`, fewer only when fewer bytes are left. With it: at most one
/// connection per 512 KiB left, starting at what the host worked with before, else
/// `initial`.
fn plan(options: &DownloadOptions, remaining: u64, host: &str) -> (usize, usize) {
    let bytes_cap = |per: u64| usize::try_from(remaining / per).unwrap_or(usize::MAX).max(1);
    match &options.adaptive {
        None => {
            let n = options.connections.min(bytes_cap(1));
            (n, n)
        }
        Some(a) => {
            let cap = options.connections.min(bytes_cap(MIN_PER_CONNECTION));
            let start = a.hosts.get(host).unwrap_or(a.initial).clamp(1, cap);
            (cap, start)
        }
    }
}

/// Failures that mean "fewer connections" or "the network had a problem".
fn is_trouble(e: &EngineError) -> bool {
    matches!(e, EngineError::HttpStatus(429 | 503) | EngineError::Network(_) | EngineError::Incomplete { .. })
}

/// Shared by the workers of one segmented download.
struct Ctx {
    source: String,
    url: Url,
    total: u64,
    validator: Option<Validator>,
    /// Sent with every range request while the server honours it. Some servers ignore
    /// `If-Range` with an ETag and send the whole file although nothing changed; then it is
    /// dropped and every 206 reply's own validator is compared instead ([`Ctx::check_206`]).
    if_range: Mutex<Option<String>>,
    destination: PathBuf,
    table: Mutex<Table>,
    counters: Arc<[AtomicU64]>,
    retry: RetryPolicy,
    /// Buffered bytes older than this are written out, so checkpoints stay current.
    flush_every: Duration,
    adaptive: Option<Adaptive>,
    budget: Option<ConnectionBudget>,
    speed_limit: Option<SpeedLimit>,
    /// Failures that suggest too many connections or a network problem (429, 503, resets,
    /// timeouts) since the controller last looked.
    trouble: AtomicU64,
}

impl Ctx {
    fn table(&self) -> MutexGuard<'_, Table> {
        // A poisoned lock only means a worker panicked; the table itself stays consistent
        // because every update is a few plain assignments.
        self.table.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Syncs the download file, then saves the bytes that were written before the sync.
    /// Without a strong validator nothing is saved: such a download can never be resumed
    /// safely, so it has no map, and `saved_state` never reports it.
    async fn checkpoint(&self) -> std::io::Result<()> {
        if self.validator.is_none() {
            return Ok(());
        }
        let done = self.table().written().to_vec();
        OpenOptions::new().write(true).open(&self.destination).await?.sync_data().await?;
        let map = RangeMap { url: self.source.clone(), size: self.total, validator: self.validator.clone(), done };
        rangemap::save(&self.destination, &map).await
    }

    fn if_range(&self) -> Option<String> {
        self.if_range.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// A 200 to a ranged request. If the reply's validator differs from ours, the file
    /// changed (If-Range did its job). If it is the same and we sent If-Range, the server
    /// does not honour If-Range: stop sending it and try the range again. Without If-Range,
    /// the server simply ignores ranges.
    /// `sent` is the If-Range this request carried (another connection may have dropped
    /// it since).
    fn whole_file_reply(&self, sent: Option<&str>, headers: &HeaderMap) -> Failure {
        match (sent, probe::validator(headers)) {
            (Some(_), Some(v)) if Some(&v) == self.validator.as_ref() => {
                *self.if_range.lock().unwrap_or_else(|e| e.into_inner()) = None;
                Failure::Retry { error: EngineError::RangeIgnored, after: Some(Duration::ZERO) }
            }
            (Some(_), _) => Failure::Fatal(EngineError::FileChanged),
            (None, _) => Failure::Fatal(EngineError::RangeIgnored),
        }
    }

    /// A 206 whose validator is not ours comes from another version of the file.
    /// While If-Range is sent, the server has already checked it, so a reply without a
    /// validator is accepted. Once it has been dropped (`sent` is `None` although we have a
    /// validator), nothing else proves the bytes are from our version: the reply must carry
    /// a validator equal to ours.
    fn check_206(&self, sent: Option<&str>, headers: &HeaderMap) -> Result<(), Failure> {
        match (probe::validator(headers), &self.validator) {
            (Some(got), Some(ours)) if &got != ours => Err(Failure::Fatal(EngineError::FileChanged)),
            (None, Some(_)) if sent.is_none() => Err(Failure::Fatal(EngineError::FileChanged)),
            _ => Ok(()),
        }
    }
}

async fn run_workers(ctx: &Arc<Ctx>, host: &str, start: usize, cancel: &CancelToken, every: Duration) -> Result<(), EngineError> {
    // Stops the other connections as soon as one fails for good.
    let abort = CancelToken::new();
    let mut tasks = JoinSet::new();
    let mut live = vec![false; ctx.counters.len()];
    let spawn = |id: usize, permit: Option<Permit>, tasks: &mut JoinSet<(usize, Result<(), EngineError>)>, live: &mut [bool]| {
        ctx.table().join();
        live[id] = true;
        let (ctx, cancel, abort) = (ctx.clone(), cancel.clone(), abort.clone());
        tasks.spawn(async move {
            let _permit = permit; // returned to the budget when this connection ends
            (id, worker(&ctx, id, &cancel, &abort).await)
        });
    };
    // The first connection waits for the budget; the others start only if it has room.
    let first = match &ctx.budget {
        None => None,
        Some(b) => tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(EngineError::Cancelled),
            p = b.acquire() => Some(p),
        },
    };
    spawn(0, first, &mut tasks, &mut live);
    for id in 1..start {
        let permit = match &ctx.budget {
            None => None,
            Some(b) => match b.try_acquire() {
                Some(p) => Some(p),
                None => break,
            },
        };
        spawn(id, permit, &mut tasks, &mut live);
    }

    let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let tick = ctx.adaptive.as_ref().map_or(Duration::from_secs(3600), |a| a.tick);
    let mut control = tokio::time::interval_at(tokio::time::Instant::now() + tick, tick);
    control.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut grow = ctx.adaptive.as_ref().map(|a| Grow::new(a, start));
    let mut failure: Option<EngineError> = None;
    let fail = |e: EngineError, failure: &mut Option<EngineError>| {
        abort.cancel();
        // Keep the first real cause, not the Cancelled errors it triggers in the others.
        if failure.as_ref().is_none_or(|f| matches!(f, EngineError::Cancelled)) {
            *failure = Some(e);
        }
    };
    // Waits for every task, so no connection is still open when this returns.
    loop {
        tokio::select! {
            joined = tasks.join_next() => match joined {
                None => break,
                Some(Ok((id, r))) => {
                    live[id] = false;
                    if let Err(e) = r {
                        fail(e, &mut failure);
                    }
                }
                Some(Err(e)) => fail(EngineError::Internal(e.to_string()), &mut failure),
            },
            _ = ticker.tick(), if failure.is_none() => {
                if let Err(e) = ctx.checkpoint().await {
                    fail(e.into(), &mut failure);
                }
            }
            _ = control.tick(), if failure.is_none() && grow.is_some() => {
                let Some(g) = grow.as_mut() else { continue };
                let received = ctx.counters.iter().map(|c| c.load(Ordering::Relaxed)).sum();
                let running = live.iter().filter(|l| **l).count();
                let trouble = ctx.trouble.swap(0, Ordering::Relaxed) > 0;
                if !g.wants_another(Instant::now(), received, running, trouble) || !ctx.table().has_work_for_another() {
                    continue;
                }
                let Some(id) = live.iter().position(|l| !*l) else { continue };
                let permit = match &ctx.budget {
                    None => None,
                    Some(b) => match b.try_acquire() {
                        Some(p) => Some(p),
                        None => continue,
                    },
                };
                g.added(running + 1);
                spawn(id, permit, &mut tasks, &mut live);
            }
        }
    }
    if let (Some(a), Some(g)) = (&ctx.adaptive, &grow)
        && failure.is_none()
        && !host.is_empty()
    {
        a.hosts.set(host, g.good);
    }
    failure.map_or(Ok(()), Err)
}

/// The adaptive controller's state for one download (specs/engine.md, "Adaptive
/// connections").
struct Grow {
    calm: Duration,
    /// The connection count that has worked so far.
    good: usize,
    /// Trouble or the start: connections are added only after `calm` (longer after
    /// additions that were followed by trouble) without any.
    quiet_since: Instant,
    /// How many additions in a row were followed by trouble; each doubles the wait.
    strikes: u32,
    /// The speed when the last connection was added, while waiting to see if it helped.
    speed_at_add: Option<f64>,
    added_at: Option<Instant>,
    /// The count after the last addition: it becomes `good` once a tick passes without
    /// trouble.
    trying: Option<usize>,
    /// More connections stopped helping: grow only back to `good`, not past it.
    plateau: bool,
    last: (Instant, u64),
}

impl Grow {
    fn new(a: &Adaptive, start: usize) -> Self {
        let now = Instant::now();
        Self {
            calm: a.calm,
            good: start,
            quiet_since: now,
            strikes: 0,
            speed_at_add: None,
            added_at: None,
            trying: None,
            plateau: false,
            last: (now, 0),
        }
    }

    /// Called every tick: should one more connection start now?
    fn wants_another(&mut self, now: Instant, received: u64, running: usize, trouble: bool) -> bool {
        let secs = now.duration_since(self.last.0).as_secs_f64().max(0.001);
        let speed = received.saturating_sub(self.last.1) as f64 / secs;
        self.last = (now, received);
        if trouble {
            // Fewer connections, or the network had a problem: what runs now is what works.
            self.good = self.good.min(running).max(1);
            if self.added_at.is_some_and(|t| now.duration_since(t) <= self.calm) {
                self.strikes = (self.strikes + 1).min(3); // the addition probably caused it
            }
            self.quiet_since = now;
            self.speed_at_add = None;
            self.added_at = None;
            self.trying = None;
            self.plateau = false;
            return false;
        }
        if let Some(n) = self.trying.take() {
            self.good = self.good.max(n);
        }
        if now.duration_since(self.quiet_since) < self.calm * 2u32.pow(self.strikes) {
            return false;
        }
        if running < self.good {
            return true; // back to what worked before the trouble
        }
        let grow = match self.speed_at_add.take() {
            Some(before) if speed < before * 1.05 => {
                // The last connection did not help: stop growing.
                self.plateau = true;
                self.good = running.saturating_sub(1).max(1);
                false
            }
            Some(_) => {
                self.strikes = 0;
                !self.plateau
            }
            None => !self.plateau,
        };
        if grow {
            self.speed_at_add = Some(speed);
        }
        grow
    }

    fn added(&mut self, now_running: usize) {
        self.trying = Some(now_running);
        self.added_at = Some(Instant::now());
    }
}

/// One attempt's failure: try again (after at least `after`), or give up.
enum Failure {
    Retry { error: EngineError, after: Option<Duration> },
    Fatal(EngineError),
}

impl From<EngineError> for Failure {
    fn from(error: EngineError) -> Self {
        if retryable(&error) { Self::Retry { error, after: None } } else { Self::Fatal(error) }
    }
}

impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Self::Fatal(e.into())
    }
}

fn status_failure(status: StatusCode, headers: &HeaderMap) -> Failure {
    let error = EngineError::HttpStatus(status.as_u16());
    if retry::retryable_status(status.as_u16()) {
        Failure::Retry { error, after: retry::retry_after(headers) }
    } else {
        Failure::Fatal(error)
    }
}

/// Bytes waiting to be written at `start`. Written by hand rather than with `BufWriter`, so
/// the worker knows exactly which bytes have reached the file when it updates the table.
struct WriteBuf {
    start: u64,
    data: Vec<u8>,
    /// When the oldest buffered byte arrived.
    since: Instant,
}

impl WriteBuf {
    async fn push(&mut self, ctx: &Ctx, file: &mut File, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
        if !self.data.is_empty() && offset != self.start + self.data.len() as u64 {
            self.flush(ctx, file).await?;
        }
        if self.data.is_empty() {
            self.start = offset;
            self.since = Instant::now();
        }
        self.data.extend_from_slice(bytes);
        if self.data.len() >= WRITE_BUFFER || self.since.elapsed() >= ctx.flush_every {
            self.flush(ctx, file).await?;
        }
        Ok(())
    }

    async fn flush(&mut self, ctx: &Ctx, file: &mut File) -> std::io::Result<()> {
        if self.data.is_empty() {
            return Ok(());
        }
        file.seek(SeekFrom::Start(self.start)).await?;
        file.write_all(&self.data).await?;
        file.flush().await?; // tokio's File finishes the write only here
        let end = self.start + self.data.len() as u64;
        ctx.table().wrote(Range { start: self.start, end });
        self.data.clear();
        Ok(())
    }
}

/// Resolves when the caller cancels or another connection failed for good.
async fn stopped(cancel: &CancelToken, abort: &CancelToken) {
    tokio::select! {
        _ = cancel.cancelled() => {}
        _ = abort.cancelled() => {}
    }
}

/// One connection: fetch a range, retrying it on its own, then take the next one.
/// Stopping is cooperative: the worker first writes out what it has buffered, so a pause
/// keeps every byte that arrived.
async fn worker(ctx: &Ctx, id: usize, cancel: &CancelToken, abort: &CancelToken) -> Result<(), EngineError> {
    let client = client()?; // its own pool, so its own TCP connection, reused range to range
    let mut file = OpenOptions::new().write(true).open(&ctx.destination).await?;
    let mut buf = WriteBuf { start: 0, data: Vec::with_capacity(WRITE_BUFFER), since: Instant::now() };
    let counter = ctx.counters.get(id).ok_or_else(|| EngineError::Internal("missing counter".into()))?;
    while ctx.table().next(id, Instant::now()).is_some() {
        let mut failures = 0u32;
        loop {
            let Some(range) = ctx.table().current(id).filter(|r| !r.is_empty()) else { break };
            let result = tokio::select! {
                biased;
                _ = stopped(cancel, abort) => Err(Failure::Fatal(EngineError::Cancelled)),
                r = fetch(ctx, &client, id, range, &mut file, &mut buf, counter) => r,
            };
            // Whatever arrived is good data at the right offset: keep it before retrying.
            buf.flush(ctx, &mut file).await?;
            let (error, after) = match result {
                Ok(()) => break,
                Err(Failure::Fatal(e)) => return Err(e),
                Err(Failure::Retry { error, after }) => {
                    if is_trouble(&error) {
                        ctx.trouble.fetch_add(1, Ordering::Relaxed);
                    }
                    // The server wants fewer connections: give the range back at once while
                    // others work; the controller adds connections again later.
                    if ctx.adaptive.is_some()
                        && matches!(error, EngineError::HttpStatus(429 | 503))
                        && ctx.table().retire(id, Instant::now())
                    {
                        return Ok(());
                    }
                    (error, after)
                }
            };
            let progressed = ctx.table().current(id).is_some_and(|r| r.start > range.start);
            failures = if progressed { 1 } else { failures + 1 };
            let wait = ctx.retry.delay(failures, after).filter(|_| failures < ctx.retry.max_attempts);
            let Some(wait) = wait else {
                // Out of retries. While other connections are still working (the server
                // may just accept fewer connections than we opened), retire this one and
                // let them take its range; only the last one to give up fails the download.
                return if ctx.table().retire(id, Instant::now()) { Ok(()) } else { Err(error) };
            };
            tokio::select! {
                biased;
                _ = stopped(cancel, abort) => return Err(EngineError::Cancelled),
                _ = tokio::time::sleep(wait) => {}
            }
        }
    }
    Ok(())
}

async fn fetch(
    ctx: &Ctx,
    client: &Client,
    id: usize,
    range: Range,
    file: &mut File,
    buf: &mut WriteBuf,
    counter: &AtomicU64,
) -> Result<(), Failure> {
    let (first, last) = (range.start, range.end - 1);
    let mut request = get(client, &ctx.url).header(RANGE, format!("bytes={first}-{last}"));
    let sent = ctx.if_range();
    if let Some(v) = &sent {
        // If the file changed, the server sends 200 and we restart rather than mix versions.
        request = request.header(IF_RANGE, v);
    }
    let mut response = request.send().await.map_err(EngineError::from)?;
    match response.status() {
        StatusCode::PARTIAL_CONTENT => ctx.check_206(sent.as_deref(), response.headers())?,
        StatusCode::OK => return Err(ctx.whole_file_reply(sent.as_deref(), response.headers())),
        s => return Err(status_failure(s, response.headers())),
    }
    let got = response.headers().get(CONTENT_RANGE).and_then(|v| v.to_str().ok()).unwrap_or("(none)");
    match parse_content_range(got) {
        Some(ContentRange::Bytes { first: f, last: l, complete }) if f == first && l == last && complete.is_none_or(|c| c == ctx.total) => {}
        _ => {
            return Err(Failure::Fatal(EngineError::BadContentRange {
                expected: format!("bytes {first}-{last}/{}", ctx.total),
                got: got.to_owned(),
            }));
        }
    }

    let mut received = 0;
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            // The connection broke mid-body (e.g. the server closed it early).
            Err(e) if e.is_body() || e.is_decode() => break,
            Err(e) => return Err(EngineError::from(e).into()),
        };
        let claim = ctx.table().claim(id, chunk.len() as u64).ok_or_else(|| Failure::Fatal(EngineError::Internal("no range".into())))?;
        buf.push(ctx, file, claim.offset, &chunk[..claim.len as usize]).await?;
        counter.fetch_add(claim.len, Ordering::Relaxed);
        received += claim.len;
        if let Some(limit) = &ctx.speed_limit {
            limit.take(claim.len).await;
        }
        if claim.finished {
            // Done, or the rest was stolen: dropping the response closes this connection
            // when it still has unread bytes; a fully read one is kept for the next range.
            return Ok(());
        }
    }
    Err(EngineError::Incomplete { expected: range.len(), received }.into())
}

/// No ranges or unknown size: one connection, in order. A retry starts from byte 0.
async fn single(info: ProbeInfo, destination: &Path, options: &DownloadOptions) -> Result<DownloadOutcome, EngineError> {
    let url = parse_url(&info.final_url)?;
    rangemap::remove(destination).await?; // nothing to resume here
    options.progress.reset(info.size, 1);
    let counters = options.progress.counters();
    let counter = counters.first().ok_or_else(|| EngineError::Internal("missing counter".into()))?;
    let policy = &options.retry;
    // One connection, front to back. With an unknown size there is no range to show.
    let _ranges = info.size.map(|size| {
        let counters = counters.clone();
        ShowRanges::new(
            &options.progress,
            RangeSource(Arc::new(move || {
                let position = counters.first().map_or(0, |c| c.load(Ordering::Relaxed)).min(size);
                vec![Some(ConnectionRange { start: 0, position, end: size })]
            })),
        )
    });

    let attempts = async {
        // Counts toward the connections shared with other downloads.
        let _permit = match &options.budget {
            Some(b) => Some(b.acquire().await),
            None => None,
        };
        let client = client()?;
        let mut failures = 0;
        loop {
            counter.store(0, Ordering::Relaxed);
            let wait = match single_attempt(&client, &url, info.size, destination, counter, options.speed_limit.as_ref()).await {
                Ok(n) => return Ok(n),
                Err(Failure::Fatal(e)) => return Err(e),
                Err(Failure::Retry { error, after }) => {
                    failures += 1;
                    match policy.delay(failures, after) {
                        Some(wait) if failures < policy.max_attempts => wait,
                        _ => return Err(error),
                    }
                }
            };
            tokio::time::sleep(wait).await;
        }
    };
    let result = tokio::select! {
        biased;
        _ = options.cancel.cancelled() => Err(EngineError::Cancelled),
        r = attempts => r,
    };
    let result = match result {
        Ok(written) => {
            let expected = info.size.unwrap_or(written);
            check_size(destination, expected, written).await.map(|()| expected)
        }
        Err(e) => Err(e),
    };
    match result {
        Ok(bytes) => {
            options.progress.set_total(bytes);
            let segments = if bytes > 0 { vec![Segment { start: 0, end: bytes - 1, received: bytes }] } else { Vec::new() };
            Ok(DownloadOutcome { probe: info, bytes, resumed: 0, segments })
        }
        Err(e) => {
            // A pre-allocated file full of zeros would look complete; never leave it behind.
            let _ = tokio::fs::remove_file(destination).await;
            Err(if options.cancel.is_cancelled() { EngineError::Cancelled } else { e })
        }
    }
}

async fn single_attempt(
    client: &Client,
    url: &Url,
    size: Option<u64>,
    destination: &Path,
    counter: &AtomicU64,
    limit: Option<&SpeedLimit>,
) -> Result<u64, Failure> {
    let file = File::create(destination).await?;
    if let Some(size) = size {
        file.set_len(size).await?;
    }
    if size == Some(0) {
        return Ok(0); // nothing to fetch
    }
    let response = get(client, url).send().await.map_err(EngineError::from)?;
    let status = response.status();
    // A plain GET must get the whole file: 206 here would be a partial body at offset 0.
    if status == StatusCode::PARTIAL_CONTENT {
        return Err(Failure::Fatal(EngineError::HttpStatus(206)));
    }
    if !status.is_success() {
        return Err(status_failure(status, response.headers()));
    }
    Ok(stream_to(response, BufWriter::with_capacity(WRITE_BUFFER, file), size, counter, limit).await?)
}

/// Copies the body into `out`, counting bytes. `expected` (when known) is enforced both ways.
async fn stream_to(
    mut response: Response,
    mut out: BufWriter<File>,
    expected: Option<u64>,
    counter: &AtomicU64,
    limit: Option<&SpeedLimit>,
) -> Result<u64, EngineError> {
    let mut received: u64 = 0;
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            // The connection broke mid-body (e.g. the server closed it early).
            Err(e) if e.is_body() || e.is_decode() => {
                return Err(match expected {
                    Some(expected) => EngineError::Incomplete { expected, received },
                    None => EngineError::from(e),
                });
            }
            Err(e) => return Err(e.into()),
        };
        let len = chunk.len() as u64;
        if expected.is_some_and(|e| received + len > e) {
            return Err(EngineError::TooMuchData);
        }
        out.write_all(&chunk).await?;
        received += len;
        counter.fetch_add(len, Ordering::Relaxed);
        if let Some(limit) = limit {
            limit.take(len).await;
        }
    }
    if let Some(expected) = expected
        && received != expected
    {
        return Err(EngineError::Incomplete { expected, received });
    }
    out.flush().await?;
    Ok(received)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covers(size: u64, segs: &[Segment]) -> bool {
        segs.first().is_some_and(|s| s.start == 0)
            && segs.last().is_some_and(|s| s.end == size - 1)
            && segs.windows(2).all(|w| w[1].start == w[0].end + 1)
    }

    #[test]
    fn split_covers_the_file_exactly() {
        for (size, n) in [(1, 16), (5, 16), (16, 16), (17, 16), (1_000_003, 32), (10, 1)] {
            let segs = split(size, n);
            assert_eq!(segs.len() as u64, (n as u64).min(size));
            assert!(covers(size, &segs), "size {size} n {n}");
            assert_eq!(segs.iter().map(Segment::len).sum::<u64>(), size);
            let (min, max) = (segs.iter().map(Segment::len).min(), segs.iter().map(Segment::len).max());
            assert!(max.unwrap() - min.unwrap() <= 1);
        }
    }
}
