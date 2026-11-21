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
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            connections: DEFAULT_CONNECTIONS,
            progress: Progress::new(),
            cancel: CancelToken::new(),
            retry: RetryPolicy::default(),
            checkpoint_interval: Duration::from_secs(2),
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
    let workers = (options.connections as u64).min(remaining.max(1)) as usize;
    let pending = if done.is_empty() {
        split(size, workers).into_iter().map(|s| Range { start: s.start, end: s.end + 1 }).collect()
    } else {
        todo
    };
    options.progress.reset(Some(size), workers);
    options.progress.set_base(resumed);

    let ctx = Arc::new(Ctx {
        source: source.to_owned(),
        url,
        total: size,
        if_range: validator.as_ref().and_then(Validator::if_range).map(str::to_owned),
        validator,
        destination: destination.to_path_buf(),
        table: Mutex::new(Table::new(workers, pending, done)),
        counters: options.progress.counters(),
        retry: options.retry.clone(),
        flush_every: options.checkpoint_interval,
    });
    // Snapshots read the table only while this download runs (also if the caller drops it).
    let weak = Arc::downgrade(&ctx);
    let _ranges = ShowRanges::new(
        &options.progress,
        RangeSource(Arc::new(move || weak.upgrade().map(|c| c.table().assignments()).unwrap_or_default())),
    );

    let mut result = match ctx.checkpoint().await {
        Ok(()) => run_workers(&ctx, workers, &options.cancel, options.checkpoint_interval).await,
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

/// Shared by the workers of one segmented download.
struct Ctx {
    source: String,
    url: Url,
    total: u64,
    validator: Option<Validator>,
    if_range: Option<String>,
    destination: PathBuf,
    table: Mutex<Table>,
    counters: Arc<[AtomicU64]>,
    retry: RetryPolicy,
    /// Buffered bytes older than this are written out, so checkpoints stay current.
    flush_every: Duration,
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

    /// A 200 to a ranged request: if the reply's validator differs from ours, the file
    /// changed (If-Range did its job); if it is the same, the server just ignores ranges.
    fn whole_file_reply(&self, headers: &HeaderMap) -> EngineError {
        match (&self.if_range, probe::validator(headers)) {
            (Some(_), Some(v)) if Some(&v) == self.validator.as_ref() => EngineError::RangeIgnored,
            (Some(_), _) => EngineError::FileChanged,
            (None, _) => EngineError::RangeIgnored,
        }
    }
}

async fn run_workers(ctx: &Arc<Ctx>, workers: usize, cancel: &CancelToken, every: Duration) -> Result<(), EngineError> {
    // Stops the other connections as soon as one fails for good.
    let abort = CancelToken::new();
    let mut tasks = JoinSet::new();
    for id in 0..workers {
        let (ctx, cancel, abort) = (ctx.clone(), cancel.clone(), abort.clone());
        tasks.spawn(async move { worker(&ctx, id, &cancel, &abort).await });
    }

    let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
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
                Some(Ok(Ok(()))) => {}
                Some(Ok(Err(e))) => fail(e, &mut failure),
                Some(Err(e)) => fail(EngineError::Internal(e.to_string()), &mut failure),
            },
            _ = ticker.tick(), if failure.is_none() => {
                if let Err(e) = ctx.checkpoint().await {
                    fail(e.into(), &mut failure);
                }
            }
        }
    }
    failure.map_or(Ok(()), Err)
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
                Err(Failure::Retry { error, after }) => (error, after),
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
    if let Some(v) = &ctx.if_range {
        // If the file changed, the server sends 200 and we restart rather than mix versions.
        request = request.header(IF_RANGE, v);
    }
    let mut response = request.send().await.map_err(EngineError::from)?;
    match response.status() {
        StatusCode::PARTIAL_CONTENT => {}
        StatusCode::OK => return Err(Failure::Fatal(ctx.whole_file_reply(response.headers()))),
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
        let client = client()?;
        let mut failures = 0;
        loop {
            counter.store(0, Ordering::Relaxed);
            let wait = match single_attempt(&client, &url, info.size, destination, counter).await {
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
    Ok(stream_to(response, BufWriter::with_capacity(WRITE_BUFFER, file), size, counter).await?)
}

/// Copies the body into `out`, counting bytes. `expected` (when known) is enforced both ways.
async fn stream_to(
    mut response: Response,
    mut out: BufWriter<File>,
    expected: Option<u64>,
    counter: &AtomicU64,
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
