---
type: Specification
title: Download engine
description: How the Rust engine downloads a file over many connections, resumes safely, and how the speed claim is proven.
tags: [engine, rust, performance]
---

# Download engine

Goal: use the user's full connection speed on every download. The headline claim is "up to 20× faster than a single-connection download"; this page says how that is achieved and how it is proven.

## Where the speed comes from
Many servers limit the speed of each connection, not of each user. A browser downloads a file over one connection, so it gets one connection's share. Opening N connections to the same file gets up to N shares, until the user's own line or disk becomes the limit.

So 20× is reachable only when the server throttles per connection, allows at least 20 connections, and the user's line has 20× headroom. On a server that already fills the line with one connection the gain is close to 1×. The engine can never exceed the line speed, and the app must never say otherwise.

## Design (Rust)
- **Async I/O:** `tokio` runtime with one lightweight task per connection, so dozens of connections cost almost nothing.
- **Probe first:** one request to learn size, range support, and a validator (`ETag` or `Last-Modified`). No range support means a single connection with no resume, shown clearly in the UI.
- **Segmented download:** split the file into byte ranges and fetch them in parallel with HTTP range requests. Default 16 connections per file, user-adjustable from 1 to 32.
- **Dynamic splitting:** when a connection finishes its range, it takes half of the largest unfinished range from the slowest connection. No connection sits idle while another is far behind, and one slow connection cannot hold up the end of the download.
- **Separate connections on purpose:** ranges go over separate TCP connections (HTTP/1.1), not multiplexed over one HTTP/2 connection, because per-connection limits apply to the whole HTTP/2 connection. Connections are kept alive and reused for the next range.
- **Adaptive connection count:** start with a few connections and add more while total speed keeps rising; stop adding when it flattens. Back off on `429`, `503`, resets or timeouts, and remember the working limit per host for the session.
- **Direct-to-place writes:** pre-allocate the final file and write each chunk at its offset. No temporary part files and no merge step at the end. Writes are buffered (about 1 MB) so the disk is not hit once per network packet.
- **Crash-safe resume:** the map of finished ranges is saved every few seconds and on pause. Resume re-validates with the stored `ETag`/`Last-Modified` (`If-Range`); if the file changed on the server, restart rather than corrupt.
- **Per-range retry:** exponential backoff with jitter; a failed range never fails the whole download until retries are exhausted.
- **Global scheduler:** a cap on total connections and on simultaneous downloads, plus an optional speed limit, so one large download does not starve the rest.
- **TLS:** `rustls`, with connection reuse to avoid repeated handshakes.

## Video sites
- yt-dlp fetches streamed video (HLS/DASH) with multiple fragments in parallel; the engine sets that fragment concurrency.
- Playlist entries download several at a time (default 3), which is where most of the playlist speed-up comes from.
- Video sites often throttle harder than file servers. The 20× claim is not made for video sites unless measured.

## Being a good client
- Never exceed 32 connections per file.
- Honour `Retry-After`.
- Back off instead of hammering a server that is refusing connections.

## Proving the claim
The claim is only published with numbers behind it.

- **Repeatable test:** a local test server that limits each connection to a fixed rate (for example 1 MB/s) and serves a 1 GB file. Measure one connection against 4, 8, 16, 20 and 32. Target: at least 19× with 20 connections.
- **Real-world test:** a fixed list of public test files, each downloaded once with a single connection and once with the engine, on the same line, back to back.
- **Recorded:** results, machine, line speed and date go in `docs/benchmarks.md`. The README claim links there.
- **Regression check:** the local test runs in CI; a drop of more than 10 % fails the build.

## Libraries (to confirm and record in THIRD_PARTY.md when the project is generated)
`tokio`, `reqwest` (or `hyper` directly if finer connection control is needed), `rustls`, `serde`.
