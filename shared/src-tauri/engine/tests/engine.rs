//! Engine tests against the local test server only (127.0.0.1, no other network).

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use common::{ETAG, RetryAfter, ServerOptions, TestServer, start};
use engine::{
    CancelToken, ConnectionRange, DEFAULT_CONNECTIONS, DownloadOptions, DownloadOutcome, EngineError, ProbeInfo,
    Progress, ProgressSnapshot, RetryPolicy, Segment, Validator, discard, download, probe, saved_state,
};

/// A fresh path in the system temp folder; removed (with its range map) when dropped.
struct TempFile(PathBuf);

impl TempFile {
    fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        Self(std::env::temp_dir().join(format!("ks-engine-test-{}-{n}.bin", std::process::id())))
    }

    fn map(&self) -> PathBuf {
        map_path(&self.0)
    }
}

fn map_path(p: &Path) -> PathBuf {
    PathBuf::from(format!("{}.ksdm", p.display()))
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.map());
        let _ = std::fs::remove_file(format!("{}.tmp", self.map().display()));
    }
}

/// Short waits, so retry tests run in milliseconds.
fn fast_retry() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 6,
        base_delay: Duration::from_millis(5),
        max_delay: Duration::from_millis(50),
        max_retry_after: Duration::from_secs(10),
    }
}

fn opts(connections: usize) -> DownloadOptions {
    DownloadOptions {
        connections,
        retry: fast_retry(),
        checkpoint_interval: Duration::from_millis(50),
        ..DownloadOptions::default()
    }
}

fn assert_same(path: &Path, expected: &[u8]) {
    let written = std::fs::read(path).expect("read result");
    assert_eq!(written.len(), expected.len(), "size");
    assert!(written == expected, "contents must be byte-for-byte identical");
}

/// Segments lie in order without gaps or overlaps and add up to `bytes`.
fn assert_tiles(segments: &[Segment], bytes: u64) {
    assert!(segments.windows(2).all(|w| w[1].start == w[0].end + 1), "{segments:?}");
    assert_eq!(segments.iter().map(Segment::len).sum::<u64>(), bytes);
    assert!(segments.iter().all(|s| s.received == s.len()));
}

async fn fetch_ok(server: &TestServer, connections: usize) -> DownloadOutcome {
    let out = TempFile::new();
    let outcome = download(&server.url, &out.0, opts(connections)).await.expect("download should succeed");
    assert_same(&out.0, &server.data());
    assert!(!out.map().exists(), "a finished download leaves no range map");
    outcome
}

/// Waits until `progress` reports at least `bytes`, or panics after 10 s.
async fn wait_for(progress: &Progress, bytes: u64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while progress.snapshot().received < bytes {
        assert!(Instant::now() < deadline, "download did not reach {bytes} bytes");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// True if `offset` lies inside one of `segments`.
fn inside(segments: &[Segment], offset: u64) -> bool {
    segments.iter().any(|s| (s.start..=s.end).contains(&offset))
}

// ---- probe ----

#[tokio::test]
async fn probe_range_capable() {
    let server = start(ServerOptions::new(1000)).await;
    let info = probe(&server.url).await.unwrap();
    assert_eq!(info.size, Some(1000));
    assert!(info.ranges);
    assert_eq!(info.validator, Some(Validator::ETag(ETAG.into())));
    assert_eq!(info.file_name, "file.bin");
}

#[test]
fn resumable_needs_ranges_a_known_size_and_a_strong_validator() {
    let base = ProbeInfo {
        final_url: "http://127.0.0.1/f".into(),
        size: Some(1000),
        ranges: true,
        validator: Some(Validator::ETag("\"v1\"".into())),
        file_name: "f".into(),
        content_type: None,
    };
    assert!(base.resumable());
    assert!(ProbeInfo { validator: Some(Validator::LastModified("Tue, 01 Dec 2026 10:00:00 GMT".into())), ..base.clone() }.resumable());
    assert!(!ProbeInfo { validator: Some(Validator::ETag("W/\"v1\"".into())), ..base.clone() }.resumable(), "weak ETag");
    assert!(!ProbeInfo { validator: None, ..base.clone() }.resumable(), "no validator");
    assert!(!ProbeInfo { ranges: false, ..base.clone() }.resumable(), "no range support");
    assert!(!ProbeInfo { size: None, ..base.clone() }.resumable(), "unknown size");
    assert!(!ProbeInfo { size: Some(0), ..base }.resumable(), "empty file");
}

#[tokio::test]
async fn probe_reports_resumable() {
    let server = start(ServerOptions::new(1000)).await;
    assert!(probe(&server.url).await.unwrap().resumable());
    let server = start(ServerOptions { etag: false, ..ServerOptions::new(1000) }).await;
    assert!(!probe(&server.url).await.unwrap().resumable());
}

#[tokio::test]
async fn probe_not_range_capable() {
    let server = start(ServerOptions { ranges: false, ..ServerOptions::new(1000) }).await;
    let info = probe(&server.url).await.unwrap();
    assert_eq!(info.size, Some(1000));
    assert!(!info.ranges);
}

#[tokio::test]
async fn probe_unknown_size() {
    let server = start(ServerOptions { ranges: false, content_length: false, ..ServerOptions::new(1000) }).await;
    let info = probe(&server.url).await.unwrap();
    assert_eq!(info.size, None);
    assert!(!info.ranges);
}

#[tokio::test]
async fn probe_works_when_head_is_rejected() {
    // The test server answers every HEAD with 405; the probe must not need HEAD.
    let server = start(ServerOptions::new(1000)).await;
    let info = probe(&server.url).await.unwrap();
    assert!(info.ranges);
    assert_eq!(server.heads.load(Ordering::SeqCst), 0, "probe must not send HEAD");
}

#[tokio::test]
async fn probe_rejects_bad_urls() {
    assert!(matches!(probe("ftp://127.0.0.1/x").await, Err(EngineError::InvalidUrl(_))));
}

// ---- segmented download ----

#[tokio::test]
async fn segmented_download_is_identical_for_many_sizes_and_connection_counts() {
    for size in [1, 1000, 65_537, 1_000_003] {
        let server = start(ServerOptions::new(size)).await;
        for connections in [1, 2, 16, 32] {
            let outcome = fetch_ok(&server, connections).await;
            // Work stealing may add ranges, never remove them.
            let at_least = (connections as u64).min(size) as usize;
            assert!(outcome.segments.len() >= at_least, "size {size}, {connections} connections");
            assert_tiles(&outcome.segments, size);
            assert_eq!(outcome.resumed, 0);
        }
    }
}

#[tokio::test]
async fn progress_reports_every_connection() {
    let server = start(ServerOptions::new(200_000)).await;
    let out = TempFile::new();
    let progress = Progress::new();
    let options = DownloadOptions { connections: 8, progress: progress.clone(), ..opts(8) };
    download(&server.url, &out.0, options).await.unwrap();
    let snap = progress.snapshot();
    assert_eq!(snap.total, Some(200_000));
    assert_eq!(snap.received, 200_000);
    assert_eq!(snap.per_connection.len(), 8);
    // 25,000 bytes per range is below the size worth splitting, so nothing is stolen.
    assert!(snap.per_connection.iter().all(|&n| n == 25_000));
}

/// Every reported range lies inside the file, its unfetched parts never overlap, and the
/// list lines up with `per_connection`.
fn assert_ranges_sane(snap: &ProgressSnapshot, size: u64) {
    assert_eq!(snap.connections.len(), snap.per_connection.len());
    let mut open: Vec<(u64, u64)> = Vec::new();
    for r in snap.connections.iter().flatten() {
        assert!(r.start <= r.position && r.position <= r.end && r.end <= size, "{r:?}");
        open.push((r.position, r.end));
    }
    open.sort();
    assert!(open.windows(2).all(|w| w[0].1 <= w[1].0), "unfetched parts overlap: {open:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn progress_reports_each_connections_current_range() {
    let size: u64 = 2_000_000;
    let server = start(ServerOptions { rate: Some(512 * 1024), ..ServerOptions::new(size) }).await;
    let out = TempFile::new();
    let progress = Progress::new();
    let options = DownloadOptions { progress: progress.clone(), ..opts(4) };
    let (url, path) = (server.url.clone(), out.0.clone());
    let task = tokio::spawn(async move { download(&url, &path, options).await });

    wait_for(&progress, size / 4).await;
    let snap = progress.snapshot();
    assert_ranges_sane(&snap, size);
    let working: Vec<ConnectionRange> = snap.connections.iter().flatten().copied().collect();
    assert!(!working.is_empty(), "connections at work report their range");
    assert!(working.iter().any(|r| r.position > r.start), "some bytes arrived: {working:?}");

    task.await.unwrap().expect("download should succeed");
    let done = progress.snapshot();
    assert_eq!(done.connections.len(), 4);
    assert!(done.connections.iter().all(Option::is_none), "no ranges once finished: {:?}", done.connections);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_download_reports_no_ranges() {
    let size: u64 = 2_000_000;
    let server = start(ServerOptions { rate: Some(128 * 1024), ..ServerOptions::new(size) }).await;
    let out = TempFile::new();
    let (progress, cancel) = (Progress::new(), CancelToken::new());
    let options = DownloadOptions { progress: progress.clone(), cancel: cancel.clone(), ..opts(4) };
    let (url, path) = (server.url.clone(), out.0.clone());
    let task = tokio::spawn(async move { download(&url, &path, options).await });
    wait_for(&progress, 1).await;
    assert!(progress.snapshot().connections.iter().any(Option::is_some));
    cancel.cancel();
    assert!(matches!(task.await.unwrap(), Err(EngineError::Cancelled)));
    assert!(progress.snapshot().connections.iter().all(Option::is_none));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn single_connection_reports_its_range_when_the_size_is_known() {
    let size: u64 = 1_000_000;
    for content_length in [true, false] {
        let server = start(ServerOptions { ranges: false, content_length, rate: Some(512 * 1024), ..ServerOptions::new(size) }).await;
        let out = TempFile::new();
        let progress = Progress::new();
        let options = DownloadOptions { progress: progress.clone(), ..opts(8) };
        let (url, path) = (server.url.clone(), out.0.clone());
        let task = tokio::spawn(async move { download(&url, &path, options).await });
        wait_for(&progress, size / 4).await;
        let snap = progress.snapshot();
        assert_ranges_sane(&snap, size);
        if content_length {
            let r = snap.connections[0].expect("one range from 0 to the end");
            assert_eq!((r.start, r.end), (0, size));
            assert!(r.position > 0);
        } else {
            assert_eq!(snap.connections, vec![None], "no range without a size");
        }
        task.await.unwrap().expect("download should succeed");
        assert_eq!(progress.snapshot().connections, vec![None]);
    }
}

// ---- fallbacks ----

#[tokio::test]
async fn single_connection_when_server_has_no_ranges() {
    let server = start(ServerOptions { ranges: false, ..ServerOptions::new(300_000) }).await;
    let outcome = fetch_ok(&server, 16).await;
    assert_eq!(outcome.segments.len(), 1);
}

#[tokio::test]
async fn single_connection_when_size_is_unknown() {
    let server = start(ServerOptions { ranges: false, content_length: false, ..ServerOptions::new(300_000) }).await;
    let outcome = fetch_ok(&server, 16).await;
    assert_eq!(outcome.bytes, 300_000);
}

#[tokio::test]
async fn tiny_file_uses_one_connection_per_byte() {
    let server = start(ServerOptions::new(5)).await;
    let outcome = fetch_ok(&server, 16).await;
    assert_eq!(outcome.segments.len(), 5);
}

#[tokio::test]
async fn zero_byte_file() {
    for ranges in [true, false] {
        let server = start(ServerOptions { ranges, ..ServerOptions::new(0) }).await;
        let outcome = fetch_ok(&server, 16).await;
        assert_eq!(outcome.bytes, 0);
    }
}

#[tokio::test]
async fn options_are_checked() {
    let out = TempFile::new();
    for n in [0, 33] {
        let r = download("http://127.0.0.1:1/x", &out.0, opts(n)).await;
        assert!(matches!(r, Err(EngineError::InvalidOptions(_))), "{n} connections");
    }
    let no_attempts = DownloadOptions { retry: RetryPolicy { max_attempts: 0, ..fast_retry() }, ..opts(4) };
    let r = download("http://127.0.0.1:1/x", &out.0, no_attempts).await;
    assert!(matches!(r, Err(EngineError::InvalidOptions(_))));
}

// ---- servers that misbehave ----

#[tokio::test]
async fn a_server_that_ignores_if_range_still_gets_a_segmented_download() {
    // Seen on a large image CDN: ranges work, but If-Range with the ETag is answered 200.
    let server = start(ServerOptions { if_range_unsupported: true, ..ServerOptions::new(300_000) }).await;
    let outcome = fetch_ok(&server, 4).await;
    assert!(outcome.segments.len() > 1, "still segmented");
    let ranged = server.range_requests();
    assert!(ranged.iter().any(|r| r.status == 206 && r.if_range.is_none()), "ranges were fetched without If-Range");
}

#[tokio::test]
async fn after_dropping_if_range_matching_validators_complete() {
    // Same server as above: every 206 carries our ETag, so the download completes.
    let server = start(ServerOptions { if_range_unsupported: true, ..ServerOptions::new(300_000) }).await;
    let outcome = fetch_ok(&server, 4).await;
    assert_eq!(outcome.resumed, 0);
    assert_eq!(server.requests().iter().filter(|r| r.probe).count(), 1, "no restart was needed");
}

#[tokio::test]
async fn after_dropping_if_range_a_206_without_validator_is_a_changed_file() {
    // Nothing proves these 206 replies come from our version: never mix them into the file.
    let server = start(ServerOptions {
        if_range_unsupported: true,
        no_validator_on_206: true,
        ..ServerOptions::new(300_000)
    })
    .await;
    let out = TempFile::new();
    let r = download(&server.url, &out.0, opts(4)).await;
    assert!(matches!(r, Err(EngineError::FileChanged)), "got {r:?}");
    assert_eq!(server.requests().iter().filter(|r| r.probe).count(), 2, "restarted once, then gave up");
    assert!(!out.0.exists(), "no mixed file is left behind");
    assert!(!out.map().exists(), "and no range map");
}

#[tokio::test]
async fn server_ignoring_range_still_gives_a_correct_file() {
    let server = start(ServerOptions { ignore_range: true, ..ServerOptions::new(300_000) }).await;
    let outcome = fetch_ok(&server, 16).await;
    assert_eq!(outcome.segments.len(), 1, "falls back to one connection");
}

#[tokio::test]
async fn whole_file_reply_to_a_range_request_fails_cleanly() {
    // The probe sees range support, but later ranges get 200 + the whole file (same ETag).
    let server = start(ServerOptions { ranges_only_for_probe: true, ..ServerOptions::new(300_000) }).await;
    let out = TempFile::new();
    let r = download(&server.url, &out.0, opts(8)).await;
    assert!(matches!(r, Err(EngineError::RangeIgnored)), "got {r:?}");
    assert!(!out.0.exists(), "no corrupt file is left behind");
    assert!(!out.map().exists(), "and no range map");
}

// ---- work stealing ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn work_stealing_rescues_a_slow_connection() {
    // Four 512 KiB ranges at 1 MiB/s each, except the one starting at 0, which gets
    // 64 KiB/s: on its own it would need 8 s. The other connections take it apart.
    let size = 2 * 1024 * 1024;
    let server = start(ServerOptions {
        rate: Some(1024 * 1024),
        slow_at: Some((0, 64 * 1024)),
        ..ServerOptions::new(size)
    })
    .await;
    let out = TempFile::new();
    let started = Instant::now();
    let outcome = download(&server.url, &out.0, opts(4)).await.expect("download");
    let took = started.elapsed();
    assert_same(&out.0, &server.data());
    assert!(took < Duration::from_secs(5), "took {took:?}; without stealing it takes 8 s");
    assert!(outcome.segments.len() > 4, "ranges were split: {:?}", outcome.segments);
    assert_tiles(&outcome.segments, size);
    let requests = server.range_requests();
    assert!(requests.len() > 4, "stolen ranges are new requests");
    assert!(
        requests.iter().filter_map(|r| r.range).any(|(a, _)| a > 0 && a < size / 4),
        "someone took part of the slow first range"
    );
}

// ---- retry ----

#[tokio::test]
async fn dropped_connections_are_retried_and_continue_where_they_stopped() {
    for connections in [1, 4] {
        // Every connection breaks after 10,000 bytes; each retry fetches the next 10,000.
        let server = start(ServerOptions { drop_after: Some(10_000), ..ServerOptions::new(200_000) }).await;
        let outcome = fetch_ok(&server, connections).await;
        assert_tiles(&outcome.segments, 200_000);
        // Each retry starts exactly where the broken connection stopped: 20 requests, at
        // 0, 10,000, 20,000 ... and none twice.
        let mut starts: Vec<u64> = server.range_requests().iter().map(|r| r.range.unwrap().0).collect();
        starts.sort_unstable();
        assert_eq!(starts, (0..20).map(|i| i * 10_000).collect::<Vec<u64>>(), "{connections} connections");
    }
}

#[tokio::test]
async fn a_range_with_no_progress_gives_up_after_max_attempts_and_stays_resumable() {
    let server = start(ServerOptions { drop_after: Some(0), ..ServerOptions::new(200_000) }).await;
    let out = TempFile::new();
    let options = DownloadOptions { retry: RetryPolicy { max_attempts: 3, ..fast_retry() }, ..opts(1) };
    let r = download(&server.url, &out.0, options).await;
    assert!(matches!(r, Err(EngineError::Incomplete { .. })), "got {r:?}");
    assert_eq!(server.range_requests().len(), 3, "exactly max_attempts tries");
    let saved = saved_state(&out.0).await.expect("map kept for a later resume");
    assert_eq!((saved.size, saved.done_bytes()), (200_000, 0));
}

#[tokio::test]
async fn retry_after_seconds_is_honoured() {
    let server = start(ServerOptions {
        fail_first: Some((1, 503, Some(RetryAfter::Seconds(1)))),
        ..ServerOptions::new(100_000)
    })
    .await;
    fetch_ok(&server, 1).await;
    let r = server.range_requests();
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].status, 503);
    let waited = r[1].at - r[0].at;
    assert!(waited >= Duration::from_secs(1), "waited only {waited:?}");
}

#[tokio::test]
async fn retry_after_http_date_is_honoured() {
    // The date has whole seconds, so "now + 2 s" means a wait of more than 1 s.
    let server = start(ServerOptions {
        fail_first: Some((1, 429, Some(RetryAfter::DateIn(2)))),
        ..ServerOptions::new(100_000)
    })
    .await;
    fetch_ok(&server, 1).await;
    let r = server.range_requests();
    assert_eq!(r[0].status, 429);
    let waited = r[1].at - r[0].at;
    assert!(waited >= Duration::from_secs(1), "waited only {waited:?}");
}

#[tokio::test]
async fn retry_after_beyond_the_limit_gives_up_at_once() {
    let server = start(ServerOptions {
        fail_first: Some((1, 503, Some(RetryAfter::Seconds(3600)))),
        ..ServerOptions::new(100_000)
    })
    .await;
    let out = TempFile::new();
    let started = Instant::now();
    let r = download(&server.url, &out.0, opts(1)).await;
    assert!(matches!(r, Err(EngineError::HttpStatus(503))), "got {r:?}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn several_failing_ranges_all_recover() {
    // The first 12 range requests fail with 503 and no Retry-After, spread over 4 connections.
    let server = start(ServerOptions { fail_first: Some((12, 503, None)), ..ServerOptions::new(400_000) }).await;
    fetch_ok(&server, 4).await;
    assert_eq!(server.range_requests().iter().filter(|r| r.status == 503).count(), 12);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_server_that_accepts_fewer_connections_still_completes_the_download() {
    // Only 3 connections at a time; the rest get 503 for as long as the others are busy, so
    // most of the 16 run out of retries. They retire and hand their ranges to the 3.
    let size: u64 = 2 * 1024 * 1024;
    let server = start(ServerOptions {
        rate: Some(2 * 1024 * 1024),
        max_connections: Some(3),
        ..ServerOptions::new(size)
    })
    .await;
    let options = DownloadOptions { retry: RetryPolicy { max_attempts: 3, ..fast_retry() }, ..opts(DEFAULT_CONNECTIONS) };
    let out = TempFile::new();
    let outcome = download(&server.url, &out.0, options).await.expect("download completes");
    assert_same(&out.0, &server.data());
    assert_tiles(&outcome.segments, size);
    assert!(server.range_requests().iter().filter(|r| r.status == 503).count() >= 13, "most connections were refused");
}

#[tokio::test]
async fn the_last_connection_to_give_up_fails_the_download() {
    // Every range request fails: each connection retires in turn until the last one fails.
    let server = start(ServerOptions { fail_first: Some((usize::MAX, 503, None)), ..ServerOptions::new(400_000) }).await;
    let out = TempFile::new();
    let options = DownloadOptions { retry: RetryPolicy { max_attempts: 2, ..fast_retry() }, ..opts(4) };
    let r = download(&server.url, &out.0, options).await;
    assert!(matches!(r, Err(EngineError::HttpStatus(503))), "got {r:?}");
    assert!(saved_state(&out.0).await.is_some(), "still resumable");
}

#[tokio::test]
async fn client_errors_are_not_retried() {
    let server = start(ServerOptions { fail_first: Some((100, 404, None)), ..ServerOptions::new(100_000) }).await;
    let out = TempFile::new();
    let r = download(&server.url, &out.0, opts(1)).await;
    assert!(matches!(r, Err(EngineError::HttpStatus(404))), "got {r:?}");
    assert_eq!(server.range_requests().len(), 1);
}

#[tokio::test]
async fn single_stream_retries_from_the_start() {
    let server = start(ServerOptions { ranges: false, drop_first: Some((2, 50_000)), ..ServerOptions::new(300_000) }).await;
    fetch_ok(&server, 4).await;
    assert_eq!(server.range_requests().len(), 3);
}

// ---- pause, crash and resume ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn crash_mid_download_then_resume_fetches_only_the_missing_bytes() {
    let size: u64 = 2 * 1024 * 1024;
    let server = start(ServerOptions { rate: Some(256 * 1024), ..ServerOptions::new(size) }).await;
    let out = TempFile::new();
    let progress = Progress::new();
    let options = DownloadOptions { progress: progress.clone(), ..opts(4) };
    let (url, path) = (server.url.clone(), out.0.clone());

    // The crash: the download runs in a runtime of its own, which is torn down mid-flight,
    // with no pause and no final save. Aborting the task alone is not a crash: file and map
    // writes run on blocking threads that outlive the abort, and one landing after the map
    // is read below would change what the resume sees (it did, under load). Dropping the
    // runtime waits for those threads, so the files on disk are final, as after a real crash.
    let crashed = std::thread::spawn({
        let progress = progress.clone();
        move || {
            let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap();
            runtime.block_on(async move {
                let task = tokio::spawn(async move { download(&url, &path, options).await });
                wait_for(&progress, size / 3).await;
                tokio::time::sleep(Duration::from_millis(120)).await; // at least one checkpoint
                task.abort();
                assert!(task.await.unwrap_err().is_cancelled());
            });
            drop(runtime);
        }
    });
    tokio::task::spawn_blocking(move || crashed.join()).await.unwrap().expect("crashed download thread");

    let saved = saved_state(&out.0).await.expect("the range map survives a crash");
    assert_eq!(saved.url, server.url);
    assert!(saved.done_bytes() > 0 && saved.done_bytes() < size, "saved {}", saved.done_bytes());
    // The saved bytes really are in the file.
    let partial = std::fs::read(&out.0).unwrap();
    for s in &saved.done {
        assert!(partial[s.start as usize..=s.end as usize] == server.data()[s.start as usize..=s.end as usize]);
    }

    server.clear_log();
    let progress = Progress::new();
    let outcome = download(&server.url, &out.0, DownloadOptions { progress: progress.clone(), ..opts(4) })
        .await
        .expect("resume");
    assert_same(&out.0, &server.data());
    assert!(!out.map().exists());
    assert_eq!(outcome.resumed, saved.done_bytes());
    assert_eq!(progress.snapshot().received, size);
    let requests = server.range_requests();
    assert!(!requests.is_empty());
    for r in &requests {
        let (first, _) = r.range.expect("ranged request");
        assert!(!inside(&saved.done, first), "resume asked again for saved bytes at {first}");
        assert_eq!(r.if_range.as_deref(), Some(ETAG), "every resumed range is validated with If-Range");
        assert_eq!(r.status, 206);
    }
    let fetched: u64 = outcome.segments.iter().map(Segment::len).sum();
    assert_eq!(fetched + outcome.resumed, size);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_server_that_stops_answering_stalls_the_download_which_resumes_later() {
    let size: u64 = 2 * 1024 * 1024;
    let server = start(ServerOptions { rate: Some(256 * 1024), ..ServerOptions::new(size) }).await;
    let out = TempFile::new();
    let progress = Progress::new();
    let stall = Duration::from_millis(600);
    let options = DownloadOptions { progress: progress.clone(), stall_timeout: stall, ..opts(4) };
    let (url, path) = (server.url.clone(), out.0.clone());
    let task = tokio::spawn(async move { download(&url, &path, options).await });

    wait_for(&progress, size / 4).await;
    server.go_silent(true);
    let silent_at = Instant::now();
    let r = tokio::time::timeout(Duration::from_secs(10), task).await.expect("gives up").expect("no panic");
    assert!(matches!(r, Err(EngineError::Stalled)), "got {r:?}");
    let took = silent_at.elapsed();
    assert!(took >= stall && took < stall * 4, "gave up after {took:?}, not the read timeout");

    // Like a pause: the partial file and its map stay, and a resume continues from them.
    let saved = saved_state(&out.0).await.expect("map kept for a resume");
    assert!(saved.done_bytes() >= size / 4 - 64 * 1024 && saved.done_bytes() < size, "saved {}", saved.done_bytes());
    server.go_silent(false);
    let outcome = download(&server.url, &out.0, opts(4)).await.expect("resume");
    assert_eq!(outcome.resumed, saved.done_bytes());
    assert_same(&out.0, &server.data());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn waiting_for_a_connection_slot_is_not_a_stall() {
    let server = start(ServerOptions { rate: Some(512 * 1024), ..ServerOptions::new(1024 * 1024) }).await;
    let budget = engine::ConnectionBudget::new(1);
    // The first download holds the only connection for about 2 s.
    let (a, b) = (TempFile::new(), TempFile::new());
    let first = DownloadOptions { budget: Some(budget.clone()), ..opts(1) };
    let (url, path) = (server.url.clone(), a.0.clone());
    let one = tokio::spawn(async move { download(&url, &path, first).await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    // The second waits for it far longer than its stall limit, and must not give up.
    let second = DownloadOptions { budget: Some(budget), stall_timeout: Duration::from_millis(300), ..opts(1) };
    let (url, path) = (server.url.clone(), b.0.clone());
    let two = tokio::spawn(async move { download(&url, &path, second).await });
    one.await.unwrap().expect("first");
    tokio::time::timeout(Duration::from_secs(10), two).await.expect("finishes").unwrap().expect("not stalled while waiting");
    assert_same(&a.0, &server.data());
    assert_same(&b.0, &server.data());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_stops_every_connection_quickly_and_keeps_the_partial_file() {
    let size: u64 = 4_000_000;
    let server = start(ServerOptions { rate: Some(64 * 1024), ..ServerOptions::new(size) }).await;
    let out = TempFile::new();
    let cancel = CancelToken::new();
    let progress = Progress::new();
    let options = DownloadOptions { progress: progress.clone(), cancel: cancel.clone(), ..opts(4) };
    let (url, path) = (server.url.clone(), out.0.clone());
    let task = tokio::spawn(async move { download(&url, &path, options).await });

    wait_for(&progress, 1).await;
    let cancelled_at = Instant::now();
    cancel.cancel();
    let r = tokio::time::timeout(Duration::from_secs(2), task).await.expect("returns promptly").expect("no panic");
    assert!(matches!(r, Err(EngineError::Cancelled)), "got {r:?}");
    assert!(cancelled_at.elapsed() < Duration::from_secs(1), "took {:?}", cancelled_at.elapsed());

    // Every connection is closed: the server sees none left.
    let deadline = Instant::now() + Duration::from_secs(2);
    while server.active.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(server.active.load(Ordering::SeqCst), 0, "all connections closed");

    // Pause keeps the file and saves everything written so far.
    assert!(out.0.exists());
    let saved = saved_state(&out.0).await.expect("map saved on pause");
    assert!(saved.done_bytes() > 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_then_resume_completes_the_same_file() {
    let size: u64 = 2 * 1024 * 1024;
    let server = start(ServerOptions { rate: Some(512 * 1024), ..ServerOptions::new(size) }).await;
    let out = TempFile::new();
    let cancel = CancelToken::new();
    let progress = Progress::new();
    let options = DownloadOptions { progress: progress.clone(), cancel: cancel.clone(), ..opts(4) };
    let (url, path) = (server.url.clone(), out.0.clone());
    let task = tokio::spawn(async move { download(&url, &path, options).await });
    wait_for(&progress, size / 4).await;
    cancel.cancel();
    assert!(matches!(task.await.unwrap(), Err(EngineError::Cancelled)));
    let saved = saved_state(&out.0).await.expect("map saved on pause");

    let progress = Progress::new();
    let outcome = download(&server.url, &out.0, DownloadOptions { progress: progress.clone(), ..opts(4) })
        .await
        .expect("resume");
    assert_same(&out.0, &server.data());
    assert_eq!(outcome.resumed, saved.done_bytes());
    assert!(outcome.resumed > 0, "what arrived before the pause was kept");
    assert_eq!(progress.snapshot().received, size);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resume_after_the_file_changed_on_the_server_starts_fresh() {
    let size: u64 = 1024 * 1024;
    let server = start(ServerOptions { rate: Some(256 * 1024), ..ServerOptions::new(size) }).await;
    let out = TempFile::new();
    let cancel = CancelToken::new();
    let progress = Progress::new();
    let options = DownloadOptions { progress: progress.clone(), cancel: cancel.clone(), ..opts(4) };
    let (url, path) = (server.url.clone(), out.0.clone());
    let task = tokio::spawn(async move { download(&url, &path, options).await });
    wait_for(&progress, size / 4).await;
    cancel.cancel();
    assert!(matches!(task.await.unwrap(), Err(EngineError::Cancelled)));
    assert!(saved_state(&out.0).await.is_some_and(|s| s.done_bytes() > 0));

    let old = server.data();
    server.change_file();
    assert_ne!(*old, *server.data());
    server.clear_log();
    let outcome = download(&server.url, &out.0, opts(4)).await.expect("fresh download");
    assert_eq!(outcome.resumed, 0, "nothing from the old file is kept");
    assert_same(&out.0, &server.data());
    assert!(server.range_requests().iter().all(|r| r.if_range.as_deref() == Some("\"v2\"")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn file_changing_during_a_download_restarts_it_with_the_new_file() {
    // Every connection breaks after 64 KiB, so new range requests (with If-Range) keep coming.
    let size: u64 = 1024 * 1024;
    let server = start(ServerOptions {
        rate: Some(512 * 1024),
        drop_after: Some(64 * 1024),
        ..ServerOptions::new(size)
    })
    .await;
    let out = TempFile::new();
    let progress = Progress::new();
    let options = DownloadOptions { progress: progress.clone(), ..opts(4) };
    let (url, path) = (server.url.clone(), out.0.clone());
    let task = tokio::spawn(async move { download(&url, &path, options).await });
    wait_for(&progress, size / 4).await;
    server.change_file();

    let outcome = task.await.unwrap().expect("download restarts and succeeds");
    assert_same(&out.0, &server.data());
    assert_eq!(outcome.resumed, 0);
    let requests = server.requests();
    assert!(
        requests.iter().any(|r| r.status == 200 && r.if_range.as_deref() == Some(ETAG)),
        "the server saw an If-Range for the old file and sent the new one"
    );
    assert_eq!(requests.iter().filter(|r| r.probe).count(), 2, "probed again before restarting");
}

#[tokio::test]
async fn a_fresh_start_removes_the_old_map_before_touching_the_file() {
    // A stale map for this very URL, next to something that cannot be the file (a folder,
    // so creating the file fails). The map must already be gone when that step fails.
    let server = start(ServerOptions::new(100_000)).await;
    let out = TempFile::new();
    std::fs::create_dir(&out.0).unwrap();
    let stale = format!(
        "ks-download-manager range map 1\nurl {}\nsize 100000\nvalidator etag {ETAG}\ndone 0 50000\nend\n",
        server.url
    );
    std::fs::write(out.map(), stale).unwrap();
    assert!(saved_state(&out.0).await.is_some());
    let r = download(&server.url, &out.0, opts(4)).await;
    std::fs::remove_dir(&out.0).unwrap();
    assert!(matches!(r, Err(EngineError::Io(_))), "got {r:?}");
    assert!(!out.map().exists(), "the stale map was removed first");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_download_without_a_validator_is_never_reported_resumable() {
    let size: u64 = 1024 * 1024;
    let server = start(ServerOptions { etag: false, rate: Some(256 * 1024), ..ServerOptions::new(size) }).await;
    let out = TempFile::new();
    let cancel = CancelToken::new();
    let progress = Progress::new();
    let options = DownloadOptions { progress: progress.clone(), cancel: cancel.clone(), ..opts(4) };
    let (url, path) = (server.url.clone(), out.0.clone());
    let task = tokio::spawn(async move { download(&url, &path, options).await });
    wait_for(&progress, size / 4).await;
    tokio::time::sleep(Duration::from_millis(120)).await; // checkpoints would have run
    assert!(!out.map().exists(), "no map is written while it runs");
    cancel.cancel();
    assert!(matches!(task.await.unwrap(), Err(EngineError::Cancelled)));
    assert!(saved_state(&out.0).await.is_none());
    assert!(!out.map().exists());
    assert!(!out.0.exists(), "a partial file that cannot be resumed is removed");
}

#[tokio::test]
async fn a_damaged_range_map_is_ignored() {
    let server = start(ServerOptions::new(300_000)).await;
    let out = TempFile::new();
    std::fs::write(&out.0, vec![0xAA; 300_000]).unwrap();
    std::fs::write(out.map(), "ks-download-manager range map 1\nurl ").unwrap();
    let outcome = download(&server.url, &out.0, opts(4)).await.unwrap();
    assert_eq!(outcome.resumed, 0);
    assert_same(&out.0, &server.data());
    assert!(!out.map().exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_range_map_for_another_url_is_ignored() {
    let size: u64 = 1024 * 1024;
    let a = start(ServerOptions { rate: Some(256 * 1024), ..ServerOptions::new(size) }).await;
    let out = TempFile::new();
    let cancel = CancelToken::new();
    let progress = Progress::new();
    let options = DownloadOptions { progress: progress.clone(), cancel: cancel.clone(), ..opts(4) };
    let (url, path) = (a.url.clone(), out.0.clone());
    let task = tokio::spawn(async move { download(&url, &path, options).await });
    wait_for(&progress, size / 4).await;
    cancel.cancel();
    let _ = task.await;
    assert!(saved_state(&out.0).await.is_some());

    // Same size and ETag, different URL: must not be stitched onto the other file.
    let b = start(ServerOptions::new(size)).await;
    let outcome = download(&b.url, &out.0, opts(4)).await.unwrap();
    assert_eq!(outcome.resumed, 0);
    assert_same(&out.0, &b.data());
}

#[tokio::test]
async fn discard_removes_the_partial_file_and_its_map() {
    let server = start(ServerOptions { drop_after: Some(0), ..ServerOptions::new(100_000) }).await;
    let out = TempFile::new();
    let options = DownloadOptions { retry: RetryPolicy { max_attempts: 1, ..fast_retry() }, ..opts(2) };
    assert!(download(&server.url, &out.0, options).await.is_err());
    assert!(out.0.exists() && out.map().exists());
    discard(&out.0).await.unwrap();
    assert!(!out.0.exists() && !out.map().exists());
    discard(&out.0).await.expect("discarding nothing is fine");
}

// ---- speed ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn more_connections_are_faster_against_a_per_connection_limit() {
    // Each connection is limited to 256 KB/s: 1 MB takes about 4 s on one connection and
    // about 0.5 s on eight. The check asks only for 4x, to stay far from flaky.
    let server = start(ServerOptions { rate: Some(256 * 1024), ..ServerOptions::new(1024 * 1024) }).await;
    let time = |connections| {
        let url = server.url.clone();
        async move {
            let out = TempFile::new();
            let t = Instant::now();
            download(&url, &out.0, opts(connections)).await.unwrap();
            t.elapsed()
        }
    };
    let one = time(1).await;
    let eight = time(8).await;
    assert!(one >= eight * 4, "1 connection {one:?}, 8 connections {eight:?}");
}

// ---- adaptive connections, shared budget, speed limit ----

/// Fast controller timings so the tests run in about a second.
fn adaptive(initial: usize) -> engine::Adaptive {
    engine::Adaptive {
        initial,
        hosts: engine::HostLimits::default(),
        tick: Duration::from_millis(100),
        calm: Duration::from_millis(300),
    }
}

fn adaptive_opts(connections: usize, a: &engine::Adaptive) -> DownloadOptions {
    DownloadOptions { adaptive: Some(a.clone()), ..opts(connections) }
}

const HOST: &str = "127.0.0.1";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn adaptive_starts_small_and_adds_connections_while_speed_rises() {
    // Each connection is throttled, so every added connection adds speed.
    let size = 8 * 1024 * 1024;
    let server = start(ServerOptions { rate: Some(1024 * 1024), ..ServerOptions::new(size) }).await;
    let a = adaptive(2);
    let progress = Progress::new();
    let out = TempFile::new();
    let task = tokio::spawn({
        let (url, path, o) = (server.url.clone(), out.0.clone(), DownloadOptions { progress: progress.clone(), ..adaptive_opts(16, &a) });
        async move { download(&url, &path, o).await }
    });
    // Early on, only the initial two are at work.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let early = progress.snapshot().connections.iter().flatten().count();
    assert!(early <= 2, "starts with 2, saw {early}");
    task.await.unwrap().expect("download");
    assert_same(&out.0, &server.data());
    let learned = a.hosts.get(HOST).expect("the host's count is remembered");
    assert!(learned > 2, "more connections were added: {learned}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn adaptive_backs_off_when_the_server_refuses_connections() {
    let size = 4 * 1024 * 1024;
    let server = start(ServerOptions { rate: Some(2 * 1024 * 1024), max_connections: Some(3), ..ServerOptions::new(size) }).await;
    let a = adaptive(6);
    let out = TempFile::new();
    download(&server.url, &out.0, adaptive_opts(8, &a)).await.expect("download");
    assert_same(&out.0, &server.data());
    assert!(server.range_requests().iter().any(|r| r.status == 503), "the server did refuse some");
    let learned = a.hosts.get(HOST).unwrap();
    assert!(learned <= 3, "remembered at most what the server allows: {learned}");

    // The next download from that host starts at the remembered count.
    server.clear_log();
    let out2 = TempFile::new();
    let progress = Progress::new();
    let task = tokio::spawn({
        let (url, path, o) = (server.url.clone(), out2.0.clone(), DownloadOptions { progress: progress.clone(), ..adaptive_opts(8, &a) });
        async move { download(&url, &path, o).await }
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(progress.snapshot().connections.iter().flatten().count() <= learned, "starts at the remembered count");
    task.await.unwrap().expect("second download");
    assert_same(&out2.0, &server.data());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn adaptive_adds_connections_back_after_a_brief_problem() {
    // The first three range requests are refused: three of the four connections retire.
    let size = 6 * 1024 * 1024;
    let server = start(ServerOptions {
        rate: Some(1024 * 1024),
        fail_first: Some((3, 503, None)),
        ..ServerOptions::new(size)
    })
    .await;
    let a = adaptive(4);
    let progress = Progress::new();
    let out = TempFile::new();
    let task = tokio::spawn({
        let (url, path, o) = (server.url.clone(), out.0.clone(), DownloadOptions { progress: progress.clone(), ..adaptive_opts(8, &a) });
        async move { download(&url, &path, o).await }
    });
    let mut low = usize::MAX;
    let mut later = 0;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !task.is_finished() && Instant::now() < deadline {
        let n = progress.snapshot().connections.iter().flatten().count();
        if n > 0 {
            low = low.min(n);
        }
        if low <= 2 {
            later = later.max(n);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    task.await.unwrap().expect("download");
    assert_same(&out.0, &server.data());
    assert!(low <= 2, "connections were lost: lowest {low}");
    assert!(later >= 3, "and came back: {later}");
}

#[tokio::test]
async fn a_tiny_file_gets_one_connection() {
    let server = start(ServerOptions::new(9_000)).await;
    let a = adaptive(4);
    let out = TempFile::new();
    let outcome = download(&server.url, &out.0, adaptive_opts(16, &a)).await.expect("download");
    assert_same(&out.0, &server.data());
    assert_eq!(outcome.segments.len(), 1);
    assert_eq!(server.range_requests().len(), 1, "one range request");
}

#[tokio::test]
async fn the_speed_limit_paces_segmented_and_single_downloads() {
    let size = 512 * 1024;
    for ranges in [true, false] {
        let server = start(ServerOptions { ranges, ..ServerOptions::new(size) }).await;
        let out = TempFile::new();
        let limit = engine::SpeedLimit::new(Some(1024 * 1024));
        let started = Instant::now();
        let o = DownloadOptions { speed_limit: Some(limit), ..opts(4) };
        download(&server.url, &out.0, o).await.expect("download");
        assert_same(&out.0, &server.data());
        let took = started.elapsed();
        assert!(took >= Duration::from_millis(400), "512 KiB at 1 MiB/s took {took:?} (ranges {ranges})");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_budget_of_one_means_one_connection() {
    let size = 2 * 1024 * 1024;
    let server = start(ServerOptions { rate: Some(8 * 1024 * 1024), ..ServerOptions::new(size) }).await;
    let budget = engine::ConnectionBudget::new(1);
    let progress = Progress::new();
    let out = TempFile::new();
    let o = DownloadOptions { budget: Some(budget.clone()), progress: progress.clone(), ..adaptive_opts(8, &adaptive(4)) };
    let task = tokio::spawn({
        let (url, path) = (server.url.clone(), out.0.clone());
        async move { download(&url, &path, o).await }
    });
    let mut most = 0;
    while !task.is_finished() {
        most = most.max(progress.snapshot().connections.iter().flatten().count());
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    task.await.unwrap().expect("download");
    assert_same(&out.0, &server.data());
    assert_eq!(most, 1, "never more than one connection");
    assert!(server.range_requests().len() >= 2, "the planned ranges were fetched one after another");
}

// ---- Small fetches (thumbnails) ----

/// Serves one fixed reply to every connection; `hang` keeps the connection open after it.
async fn raw_server(reply: &'static str, hang: bool) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf).await;
                let _ = s.write_all(reply.as_bytes()).await;
                if hang {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                }
            });
        }
    });
    format!("http://{addr}/thumb.jpg")
}

#[tokio::test]
async fn a_small_fetch_never_follows_a_redirect() {
    let url = raw_server("HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", false).await;
    let client = engine::SmallClient::new(Duration::from_secs(5)).unwrap();
    let r = client.get(&url, 1024).await;
    assert!(matches!(r, Err(EngineError::HttpStatus(302))), "got {r:?}");
}

#[tokio::test]
async fn a_small_fetch_has_an_overall_time_limit_and_a_size_limit() {
    // Headers at once, then a body that never comes.
    let slow = raw_server("HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nabc", true).await;
    let client = engine::SmallClient::new(Duration::from_millis(300)).unwrap();
    let t = Instant::now();
    assert!(client.get(&slow, 1024).await.is_err());
    assert!(t.elapsed() < Duration::from_secs(3), "took {:?}", t.elapsed());

    let ok = raw_server("HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello", false).await;
    assert_eq!(client.get(&ok, 1024).await.unwrap(), b"hello");
    assert!(matches!(client.get(&ok, 4).await, Err(EngineError::TooMuchData)));
}
