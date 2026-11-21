use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use reqwest::header::{CONTENT_RANGE, IF_RANGE, RANGE};
use reqwest::{Response, StatusCode, Url};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, BufWriter};
use tokio::task::JoinSet;

use crate::control::{CancelToken, Progress};
use crate::error::EngineError;
use crate::http::{ContentRange, client, get, parse_content_range, parse_url};
use crate::probe::{ProbeInfo, Validator, probe};

/// Connections per file unless the caller asks otherwise (specs/engine.md).
pub const DEFAULT_CONNECTIONS: usize = 16;
/// Never more than this many connections for one file (specs/engine.md, "Being a good client").
pub const MAX_CONNECTIONS: usize = 32;
/// Each connection buffers about this much before writing, so the disk is not hit per packet.
const WRITE_BUFFER: usize = 1 << 20;

#[derive(Clone, Debug)]
pub struct DownloadOptions {
    /// 1 to 32. Fewer are used when the file has fewer bytes, or the server has no range support.
    pub connections: usize,
    /// Poll `progress.snapshot()` from anywhere while the download runs.
    pub progress: Progress,
    /// Call `cancel.cancel()` to stop every connection; the download returns `Cancelled`.
    pub cancel: CancelToken,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self { connections: DEFAULT_CONNECTIONS, progress: Progress::new(), cancel: CancelToken::new() }
    }
}

/// One byte range of the file, inclusive at both ends, and how much of it arrived.
/// Kept as plain data so Stage 3 can persist it as the range map for resume.
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

#[derive(Clone, Debug)]
pub struct DownloadOutcome {
    pub probe: ProbeInfo,
    /// Bytes written; equals the file size.
    pub bytes: u64,
    /// The ranges used; one segment covering the file when a single connection was used.
    pub segments: Vec<Segment>,
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

/// Probes `url`, then downloads it to `destination` (created or overwritten).
///
/// With range support and a known size, the file is pre-allocated and fetched over
/// `options.connections` separate connections, each writing at its own offset. Otherwise
/// one connection writes it in order. On failure or cancel the partial file is removed
/// (Stage 3 keeps it for resume).
pub async fn download(url: &str, destination: &Path, options: DownloadOptions) -> Result<DownloadOutcome, EngineError> {
    let DownloadOptions { connections, progress, cancel } = options;
    if !(1..=MAX_CONNECTIONS).contains(&connections) {
        return Err(EngineError::InvalidOptions(format!("connections must be 1 to {MAX_CONNECTIONS}, got {connections}")));
    }
    let info = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(EngineError::Cancelled),
        r = probe(url) => r?,
    };
    let url = parse_url(&info.final_url)?;
    let plan = match info.size {
        Some(size) if info.ranges && size > 0 && connections > 1 => Some(split(size, connections)),
        _ => None,
    };
    progress.reset(info.size, plan.as_ref().map_or(1, Vec::len));

    let file = tokio::fs::File::create(destination).await?;
    if let Some(size) = info.size {
        file.set_len(size).await?; // pre-allocate; ranges write straight into place
    }
    drop(file);

    let result = match (&plan, info.size) {
        (_, Some(0)) => Ok(0), // nothing to fetch
        (Some(segments), Some(size)) => {
            run_segments(&url, segments, size, info.validator.as_ref(), destination, &progress, &cancel).await
        }
        _ => run_single(&url, info.size, destination, &progress, &cancel).await,
    };

    let finish = async {
        let written = result?;
        let expected = info.size.unwrap_or(written);
        let on_disk = tokio::fs::metadata(destination).await?.len();
        if written != expected || on_disk != expected {
            return Err(EngineError::SizeMismatch { expected, written: written.min(on_disk) });
        }
        progress.set_total(expected);
        Ok(expected)
    };
    match finish.await {
        Ok(bytes) => {
            let counts = progress.snapshot().per_connection;
            let segments = match plan {
                Some(mut segs) => {
                    segs.iter_mut().zip(counts).for_each(|(s, c)| s.received = c);
                    segs
                }
                None if bytes > 0 => vec![Segment { start: 0, end: bytes - 1, received: bytes }],
                None => Vec::new(),
            };
            Ok(DownloadOutcome { probe: info, bytes, segments })
        }
        Err(e) => {
            // A pre-allocated file full of zeros would look complete; never leave it behind.
            let _ = tokio::fs::remove_file(destination).await;
            Err(if cancel.is_cancelled() { EngineError::Cancelled } else { e })
        }
    }
}

async fn run_segments(
    url: &Url,
    segments: &[Segment],
    total: u64,
    validator: Option<&Validator>,
    destination: &Path,
    progress: &Progress,
    cancel: &CancelToken,
) -> Result<u64, EngineError> {
    // Stops the other connections as soon as one fails (no retry until Stage 3).
    let abort = CancelToken::new();
    let counters = progress.counters();
    let mut tasks = JoinSet::new();
    for (index, segment) in segments.iter().copied().enumerate() {
        let job = RangeJob {
            url: url.clone(),
            segment,
            total,
            if_range: validator.and_then(Validator::if_range).map(str::to_owned),
            destination: destination.to_path_buf(),
            counters: counters.clone(),
            index,
        };
        let (cancel, abort) = (cancel.clone(), abort.clone());
        tasks.spawn(async move {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => Err(EngineError::Cancelled),
                _ = abort.cancelled() => Err(EngineError::Cancelled),
                r = job.run() => r,
            }
        });
    }

    let mut written = 0;
    let mut failure: Option<EngineError> = None;
    // Waits for every task, so no connection is still open when this returns.
    while let Some(joined) = tasks.join_next().await {
        match joined.unwrap_or_else(|e| Err(EngineError::Internal(e.to_string()))) {
            Ok(n) => written += n,
            Err(e) => {
                abort.cancel();
                // Keep the first real cause, not the Cancelled errors it triggers in the others.
                if failure.as_ref().is_none_or(|f| matches!(f, EngineError::Cancelled)) {
                    failure = Some(e);
                }
            }
        }
    }
    match failure {
        Some(e) => Err(e),
        None => Ok(written),
    }
}

struct RangeJob {
    url: Url,
    segment: Segment,
    total: u64,
    if_range: Option<String>,
    destination: PathBuf,
    counters: Arc<[AtomicU64]>,
    index: usize,
}

impl RangeJob {
    async fn run(self) -> Result<u64, EngineError> {
        let Segment { start, end, .. } = self.segment;
        let client = client()?;
        let mut request = get(&client, &self.url).header(RANGE, format!("bytes={start}-{end}"));
        if let Some(v) = &self.if_range {
            // If the file changed since the probe, the server sends 200 and we stop
            // instead of mixing two versions of the file.
            request = request.header(IF_RANGE, v);
        }
        let response = request.send().await?;
        match response.status() {
            StatusCode::PARTIAL_CONTENT => {}
            StatusCode::OK => return Err(EngineError::RangeIgnored),
            s => return Err(EngineError::HttpStatus(s.as_u16())),
        }
        let got = response.headers().get(CONTENT_RANGE).and_then(|v| v.to_str().ok()).unwrap_or("(none)");
        match parse_content_range(got) {
            Some(ContentRange::Bytes { first, last, complete })
                if first == start && last == end && complete.is_none_or(|c| c == self.total) => {}
            _ => {
                return Err(EngineError::BadContentRange {
                    expected: format!("bytes {start}-{end}/{}", self.total),
                    got: got.to_owned(),
                });
            }
        }

        let mut file = OpenOptions::new().write(true).open(&self.destination).await?;
        file.seek(SeekFrom::Start(start)).await?;
        let counter = self.counters.get(self.index).ok_or_else(|| EngineError::Internal("missing counter".into()))?;
        stream_to(response, BufWriter::with_capacity(WRITE_BUFFER, file), Some(self.segment.len()), counter).await
    }
}

async fn run_single(
    url: &Url,
    size: Option<u64>,
    destination: &Path,
    progress: &Progress,
    cancel: &CancelToken,
) -> Result<u64, EngineError> {
    let job = async {
        let client = client()?;
        let response = get(&client, url).send().await?;
        let status = response.status();
        // A plain GET must get the whole file: 206 here would be a partial body at offset 0.
        if !status.is_success() || status == StatusCode::PARTIAL_CONTENT {
            return Err(EngineError::HttpStatus(status.as_u16()));
        }
        let file = OpenOptions::new().write(true).open(destination).await?;
        let counters = progress.counters();
        let counter = counters.first().ok_or_else(|| EngineError::Internal("missing counter".into()))?;
        stream_to(response, BufWriter::with_capacity(WRITE_BUFFER, file), size, counter).await
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(EngineError::Cancelled),
        r = job => r,
    }
}

/// Copies the body into `out`, counting bytes. `expected` (when known) is enforced both ways.
async fn stream_to(
    mut response: Response,
    mut out: BufWriter<tokio::fs::File>,
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
