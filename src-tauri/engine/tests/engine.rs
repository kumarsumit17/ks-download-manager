//! Engine tests against the local test server only (127.0.0.1, no other network).

mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use common::{ETAG, ServerOptions, start};
use engine::{CancelToken, DownloadOptions, EngineError, Progress, Validator, download, probe};

/// A fresh path in the system temp folder; removed when dropped.
struct TempFile(PathBuf);

impl TempFile {
    fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        Self(std::env::temp_dir().join(format!("ks-engine-test-{}-{n}.bin", std::process::id())))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn opts(connections: usize) -> DownloadOptions {
    DownloadOptions { connections, ..DownloadOptions::default() }
}

async fn fetch_ok(server: &common::TestServer, connections: usize) -> engine::DownloadOutcome {
    let out = TempFile::new();
    let outcome = download(&server.url, &out.0, opts(connections)).await.expect("download should succeed");
    let written = std::fs::read(&out.0).expect("read result");
    assert_eq!(written.len(), server.data.len(), "size");
    assert!(written == *server.data, "contents must be byte-for-byte identical");
    outcome
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
            let expected = (connections as u64).min(size) as usize;
            assert_eq!(outcome.segments.len(), expected, "size {size}, {connections} connections");
            assert!(outcome.segments.iter().all(|s| s.received == s.len()));
        }
    }
}

#[tokio::test]
async fn progress_reports_every_connection() {
    let server = start(ServerOptions::new(200_000)).await;
    let out = TempFile::new();
    let progress = Progress::new();
    let options = DownloadOptions { connections: 8, progress: progress.clone(), ..DownloadOptions::default() };
    download(&server.url, &out.0, options).await.unwrap();
    let snap = progress.snapshot();
    assert_eq!(snap.total, Some(200_000));
    assert_eq!(snap.received, 200_000);
    assert_eq!(snap.per_connection.len(), 8);
    assert!(snap.per_connection.iter().all(|&n| n == 25_000));
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
async fn connection_count_is_checked() {
    let out = TempFile::new();
    for n in [0, 33] {
        let r = download("http://127.0.0.1:1/x", &out.0, opts(n)).await;
        assert!(matches!(r, Err(EngineError::InvalidOptions(_))), "{n} connections");
    }
}

// ---- servers that misbehave ----

#[tokio::test]
async fn server_ignoring_range_still_gives_a_correct_file() {
    let server = start(ServerOptions { ignore_range: true, ..ServerOptions::new(300_000) }).await;
    let outcome = fetch_ok(&server, 16).await;
    assert_eq!(outcome.segments.len(), 1, "falls back to one connection");
}

#[tokio::test]
async fn whole_file_reply_to_a_range_request_fails_cleanly() {
    // The probe sees range support, but later ranges get 200 + the whole file.
    let server = start(ServerOptions { ranges_only_for_probe: true, ..ServerOptions::new(300_000) }).await;
    let out = TempFile::new();
    let r = download(&server.url, &out.0, opts(8)).await;
    assert!(matches!(r, Err(EngineError::RangeIgnored)), "got {r:?}");
    assert!(!out.0.exists(), "no corrupt file is left behind");
}

#[tokio::test]
async fn dropped_connection_fails_with_incomplete() {
    for connections in [1, 4] {
        let server = start(ServerOptions { drop_after: Some(10_000), ..ServerOptions::new(200_000) }).await;
        let out = TempFile::new();
        let r = download(&server.url, &out.0, opts(connections)).await;
        assert!(
            matches!(r, Err(EngineError::Incomplete { .. })),
            "{connections} connections: got {r:?}"
        );
        assert!(!out.0.exists(), "partial file is removed");
    }
}

// ---- cancel ----

#[tokio::test]
async fn cancel_stops_every_connection_quickly() {
    let server = start(ServerOptions { rate: Some(64 * 1024), ..ServerOptions::new(4_000_000) }).await;
    let out = TempFile::new();
    let cancel = CancelToken::new();
    let progress = Progress::new();
    let options = DownloadOptions { connections: 4, progress: progress.clone(), cancel: cancel.clone() };
    let url = server.url.clone();
    let path = out.0.clone();
    let task = tokio::spawn(async move { download(&url, &path, options).await });

    // Wait until data is flowing, then cancel.
    let deadline = Instant::now() + Duration::from_secs(5);
    while progress.snapshot().received == 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(progress.snapshot().received > 0, "download should have started");
    let cancelled_at = Instant::now();
    cancel.cancel();
    let r = tokio::time::timeout(Duration::from_secs(2), task).await.expect("returns promptly").expect("no panic");
    assert!(matches!(r, Err(EngineError::Cancelled)), "got {r:?}");
    assert!(cancelled_at.elapsed() < Duration::from_secs(1), "took {:?}", cancelled_at.elapsed());
    assert!(!out.0.exists(), "partial file is removed");

    // Every connection is closed: the server sees none left.
    let deadline = Instant::now() + Duration::from_secs(2);
    while server.active.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(server.active.load(Ordering::SeqCst), 0, "all connections closed");
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
