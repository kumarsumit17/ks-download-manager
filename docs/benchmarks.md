# Benchmarks

How the download engine scales with the number of connections ([engine spec](../specs/engine.md#proving-the-claim)). **Read the two tables together:** the first shows what more connections give when a server limits each connection; the second shows what they gave on a real line, where one connection was already enough to fill it.

## 1. Local, per-connection throttled server (repeatable)

**What it measures:** how the engine scales when a server limits each connection, **not real-world speed**. A local server (`src-tauri/engine/tests/common/mod.rs`) serves one 64 MiB file and limits every connection to 1 MiB/s; the engine downloads it with a fixed number of connections. **Adaptive connections are off**, so each row uses exactly that many connections from the start (no ramp-up). Time runs from the probe to the finished, checked file.

- **Run:** `npm run bench` (release build; about 100 s). `KSDM_BENCH_MIB` and `KSDM_BENCH_RATE_KIB` change the file size and the per-connection rate.
- **Machine:** Mac17,3, Apple M5 (10 cores), 16 GB, macOS 27.2.1. Loopback only, so the line speed plays no part.
- **Date:** 2026-11-21. Two runs, identical to two decimals; the second is shown.

| Connections | Time (s) | Throughput (MiB/s) | Speed-up | Ideal |
|---:|---:|---:|---:|---:|
| 1 | 64.00 | 1.00 | 1.00× | 1× |
| 4 | 16.00 | 4.00 | 4.00× | 4× |
| 8 | 8.00 | 8.00 | 8.00× | 8× |
| 16 | 4.01 | 15.97 | 15.97× | 16× |
| 20 | 3.21 | 19.94 | 19.95× | 20× |
| 32 | 2.01 | 31.77 | 31.78× | 32× |

The spec's target, at least 19× with 20 connections, is met (19.95×). The spec describes a 1 GB file at 1 MB/s per connection; the single-connection run alone would take 17 minutes, so the file is 64 MiB at the same rate per connection. The overhead that can show (probe, splitting, the last range finishing) is a fixed cost, so it weighs a little more in a smaller file.

**Regression check (ready for CI):** the same run fails when
- a speed-up drops more than 10 % below `src-tauri/engine/tests/bench-baseline.txt` (the spec's rule), or a measured count is missing from the baseline;
- the one-connection run does not take size ÷ rate within 5 % (64 s here): otherwise the throttle did not work and the ratios would measure nothing;
- an **unthrottled** run (256 MiB over 8 connections from the same local server, no limit) stays below a floor of 500 MiB/s (`KSDM_BENCH_FLOOR_MIBS`). Speed-ups alone cannot catch an engine that got slower everywhere; the floor can. Measured here: **3,095 MiB/s**; the floor is about a sixth of that, so slower machines pass.

Speed-ups, not times, are compared with the baseline, because under a per-connection limit they do not depend on the machine. A new baseline is written with `KSDM_BENCH_RECORD=1 npm run bench` (the time and floor checks apply to that run too, so a broken run is never recorded). Added after the final review. No CI workflow exists yet (Release stage); a workflow only needs to run `npm run bench` on macOS or Linux with Rust installed.

## 2. Real world (one line, one day)

**What it measures:** one public speed-test file downloaded in full with one connection, then as the app does it (adaptive connections, at most 16), back to back on the same line.

- **Run:** `cargo test --release --manifest-path src-tauri/Cargo.toml -p engine --test bench real_world -- --ignored --nocapture` (`KSDM_BENCH_URLS` replaces the list).
- **Line:** the development Mac's connection; one connection reached about 49 Mbit/s. Date 2026-11-21.

| File | Size | 1 connection | Engine (adaptive) | Speed-up |
|---|---:|---:|---:|---:|
| `https://proof.ovh.net/files/100Mb.dat` | 100 MiB | 5.84 MiB/s | 6.22 MiB/s | 1.06× |
| `http://speedtest.tele2.net/100MB.zip` | — | could not connect (timed out) | — | — |

One connection already used about all of this line, so more connections could add little: 1.06×. This is the case the spec describes: "On a server that already fills the line with one connection the gain is close to 1×." A faster line, or a server that limits each connection, is needed to see more.

## What may be said
- **Supported:** "On a server that limits each connection, 20 connections were 19.95× as fast as one (local test)."
- **Not supported:** any general "up to 20× faster" claim. On the one real line measured, the gain was 1.06×. The README and the app say no more than the local result with this condition, and point here.
