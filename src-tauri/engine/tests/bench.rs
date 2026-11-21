//! The engine benchmark (specs/engine.md, "Proving the claim"; results in docs/benchmarks.md).
//!
//! A local server limits every connection to the same rate and serves one file; the engine
//! downloads it with 1, 4, 8, 16, 20 and 32 connections, adaptive connections off, so each
//! count is exactly the one measured. Each run is timed from the probe to the finished file.
//! The speed-up is the throughput against one connection. This measures how the engine
//! scales when a server throttles each connection, not real-world speed: a server that
//! fills the line with one connection gains nothing.
//!
//! Regression check: every speed-up must stay within 10 % of `bench-baseline.txt` (next to
//! this file). Run in release mode:
//!   npm run bench                       (check against the baseline)
//!   KSDM_BENCH_RECORD=1 npm run bench   (write a new baseline)
//! Size and rate: KSDM_BENCH_MIB (default 64) and KSDM_BENCH_RATE_KIB (per connection,
//! default 1024).
//!
//! The real-world test (`real_world_single_against_engine`, also ignored) downloads public
//! speed-test files once with one connection and once as the app does (16 connections at
//! most, adaptive), back to back, on whatever line the machine has:
//!   cargo test --release --manifest-path src-tauri/Cargo.toml -p engine --test bench real_world -- --ignored --nocapture
//! KSDM_BENCH_URLS (space-separated) replaces the default list.

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{ServerOptions, start};
use engine::{Adaptive, DownloadOptions, RetryPolicy, download};

const COUNTS: [usize; 6] = [1, 4, 8, 16, 20, 32];
/// A speed-up may drop this much below the baseline before the check fails.
const ALLOWED_DROP: f64 = 0.10;

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/bench-baseline.txt")
}

/// `connections speed-up` per line; `#` starts a comment.
fn read_baseline() -> Vec<(usize, f64)> {
    let Ok(text) = std::fs::read_to_string(baseline_path()) else { return Vec::new() };
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| {
            let mut p = l.split_whitespace();
            Some((p.next()?.parse().ok()?, p.next()?.parse().ok()?))
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "takes about two minutes; run with `npm run bench`"]
async fn benchmark_connections() {
    let size = env_u64("KSDM_BENCH_MIB", 64) * 1024 * 1024;
    let rate = env_u64("KSDM_BENCH_RATE_KIB", 1024) * 1024;
    let server = start(ServerOptions { rate: Some(rate), ..ServerOptions::new(size) }).await;
    let dir = std::env::temp_dir().join(format!("ksdm-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    println!("file {} MiB, {} KiB/s per connection, adaptive connections off", size >> 20, rate >> 10);
    println!("| Connections | Time (s) | Throughput (MiB/s) | Speed-up | Ideal |");
    println!("|---:|---:|---:|---:|---:|");
    let mut results: Vec<(usize, f64)> = Vec::new();
    let mut single = 0.0;
    for n in COUNTS {
        let path = dir.join(format!("bench-{n}.bin"));
        let options = DownloadOptions {
            connections: n,
            retry: RetryPolicy::default(),
            checkpoint_interval: Duration::from_secs(2),
            ..DownloadOptions::default()
        };
        let t = Instant::now();
        let outcome = download(&server.url, &path, options).await.expect("download");
        let secs = t.elapsed().as_secs_f64();
        assert_eq!(outcome.bytes, size);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), size);
        std::fs::remove_file(&path).unwrap();
        let throughput = size as f64 / secs / (1024.0 * 1024.0);
        if n == 1 {
            single = throughput;
        }
        let speedup = throughput / single;
        println!("| {n} | {secs:.2} | {throughput:.2} | {speedup:.2}× | {n}× |");
        results.push((n, speedup));
    }
    let _ = std::fs::remove_dir_all(&dir);

    if std::env::var_os("KSDM_BENCH_RECORD").is_some() {
        let mut text = format!(
            "# Engine benchmark baseline: connections, speed-up against one connection.\n# {} MiB file, {} KiB/s per connection, adaptive off. Written by KSDM_BENCH_RECORD=1 npm run bench.\n",
            size >> 20,
            rate >> 10
        );
        for (n, s) in &results {
            text.push_str(&format!("{n} {s:.2}\n"));
        }
        std::fs::write(baseline_path(), text).unwrap();
        println!("baseline written to {}", baseline_path().display());
        return;
    }
    let baseline = read_baseline();
    assert!(!baseline.is_empty(), "no baseline: run `KSDM_BENCH_RECORD=1 npm run bench` first");
    let mut failed = Vec::new();
    for (n, s) in &results {
        if let Some((_, b)) = baseline.iter().find(|(m, _)| m == n)
            && *s < b * (1.0 - ALLOWED_DROP)
        {
            failed.push(format!("{n} connections: {s:.2}× against {b:.2}×"));
        }
    }
    assert!(failed.is_empty(), "speed-up dropped more than 10 %: {}", failed.join("; "));
}

/// Public files published for speed tests; downloaded in full, twice each.
const REAL_WORLD: &[&str] = &["https://proof.ovh.net/files/100Mb.dat", "http://speedtest.tele2.net/100MB.zip"];

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "downloads about 400 MB from public test servers"]
async fn real_world_single_against_engine() {
    let urls: Vec<String> = match std::env::var("KSDM_BENCH_URLS") {
        Ok(v) => v.split_whitespace().map(str::to_owned).collect(),
        Err(_) => REAL_WORLD.iter().map(|s| (*s).to_owned()).collect(),
    };
    let dir = std::env::temp_dir().join(format!("ksdm-real-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    println!("| File | Size (MiB) | 1 connection (MiB/s) | Engine, adaptive (MiB/s) | Speed-up |");
    println!("|---|---:|---:|---:|---:|");
    for url in &urls {
        let mut speeds = Vec::new();
        let mut size = 0;
        for adaptive in [false, true] {
            let path = dir.join("real.bin");
            let options = if adaptive {
                DownloadOptions { connections: 16, adaptive: Some(Adaptive::default()), ..DownloadOptions::default() }
            } else {
                DownloadOptions { connections: 1, ..DownloadOptions::default() }
            };
            let t = Instant::now();
            match download(url, &path, options).await {
                Ok(o) => {
                    size = o.bytes;
                    speeds.push(o.bytes as f64 / t.elapsed().as_secs_f64() / (1024.0 * 1024.0));
                }
                Err(e) => {
                    println!("| {url} | failed: {e} | | | |");
                    break;
                }
            }
            let _ = std::fs::remove_file(&path);
            let _ = engine::discard(&path).await;
        }
        if let [one, many] = speeds[..] {
            println!("| {url} | {:.0} | {one:.2} | {many:.2} | {:.2}× |", size as f64 / 1048576.0, many / one);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
