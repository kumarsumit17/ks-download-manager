//! Try the engine by hand:
//!
//!     cargo run -p engine --example fetch -- <url> <output-file> [connections]
//!
//! Prints size, connections, live progress and the average speed. Ctrl+C stops the program.

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use engine::{DEFAULT_CONNECTIONS, DownloadOptions, Progress, download, probe};

fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(url), Some(out)) = (args.first(), args.get(1)) else {
        eprintln!("usage: fetch <url> <output-file> [connections 1-32, default {DEFAULT_CONNECTIONS}]");
        std::process::exit(2);
    };
    let connections = match args.get(2).map(|c| c.parse::<usize>()) {
        None => DEFAULT_CONNECTIONS,
        Some(Ok(n)) => n,
        Some(Err(_)) => {
            eprintln!("connections must be a number from 1 to 32");
            std::process::exit(2);
        }
    };

    match probe(url).await {
        Ok(info) => println!(
            "file: {}\nsize: {}\nranges: {}\nconnections asked: {connections}",
            info.file_name,
            info.size.map_or("unknown".into(), |s| format!("{s} bytes ({:.1} MB)", mb(s))),
            if info.ranges { "yes" } else { "no (one connection)" },
        ),
        Err(e) => {
            eprintln!("probe failed: {e}");
            std::process::exit(1);
        }
    }

    let progress = Progress::new();
    let options = DownloadOptions { connections, progress: progress.clone(), ..DownloadOptions::default() };
    let started = Instant::now();
    let path = PathBuf::from(out);
    let owned_url = url.clone();
    let task = tokio::spawn(async move { download(&owned_url, &path, options).await });

    let mut tick = tokio::time::interval(Duration::from_millis(500));
    while !task.is_finished() {
        tick.tick().await;
        let s = progress.snapshot();
        let pct = s.total.filter(|&t| t > 0).map_or(String::new(), |t| format!(" {:5.1}%", s.received as f64 * 100.0 / t as f64));
        let speed = mb(s.received) / started.elapsed().as_secs_f64().max(0.001);
        let active = s.per_connection.len();
        print!("\r{:8.1} MB{pct}  {speed:7.2} MB/s  {active} connection(s)   ", mb(s.received));
        let _ = std::io::stdout().flush();
    }
    println!();

    match task.await {
        Ok(Ok(outcome)) => {
            let secs = started.elapsed().as_secs_f64();
            println!(
                "done: {} bytes over {} connection(s) in {secs:.1} s, average {:.2} MB/s",
                outcome.bytes,
                outcome.segments.len().max(1),
                mb(outcome.bytes) / secs.max(0.001)
            );
            println!("check it with: shasum -a 256 {out}");
        }
        Ok(Err(e)) => {
            eprintln!("download failed: {e}");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("download task stopped: {e}");
            std::process::exit(1);
        }
    }
}
