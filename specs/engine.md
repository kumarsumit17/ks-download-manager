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
- **Direct-to-place writes:** pre-allocate the destination file and write each chunk at its offset. No per-connection part files and no merge step at the end. Writes are buffered (about 1 MB) so the disk is not hit once per network packet.
- **In-progress name (app):** the app gives the engine `<final name>.ksdownload` as the destination, so its range map is `<final name>.ksdownload.ksdm`, and renames it to the final name when the download succeeds. An unfinished file never carries the real name, so it cannot be mistaken for a complete one. The engine itself writes wherever it is told.
- **Crash-safe resume:** the map of finished ranges is saved every few seconds and on pause. Resume re-validates with the stored `ETag`/`Last-Modified` (`If-Range`); if the file changed on the server, restart rather than corrupt.
- **Per-range retry:** exponential backoff with jitter; a failed range never fails the whole download until retries are exhausted.
- **Global scheduler:** a cap on total connections and on simultaneous downloads, plus an optional speed limit, so one large download does not starve the rest.
- **TLS:** `rustls`, with connection reuse to avoid repeated handshakes.

## Splitting, retry and resume in detail
- **Victim choice:** an idle connection takes the upper half of the range with the longest estimated time left (bytes left ÷ that connection's measured speed; a range that has just started uses its connection's speed on its previous range). Ranges under 128 KB are not split. A connection claims bytes before writing them, so no byte is ever fetched by two connections.
- **Retry policy:** network errors, connections that end early, and HTTP 408, 429, 500, 502, 503 and 504 are retried; other statuses are final. Backoff is exponential with full jitter (default 500 ms doubling to 30 s), a retry continues from the first missing byte, and 6 attempts in a row without new bytes end that connection's attempts on the range. `Retry-After` (seconds or any HTTP-date form) is a lower bound on the wait; a wait above 10 minutes ends the retries.
- **Servers that ignore `If-Range`:** some CDNs answer a range request carrying `If-Range` with the ETag with 200 and the whole file although nothing changed (same validator). The download then stops sending `If-Range` and compares the validator of every 206 reply instead: from then on each 206 must carry a validator equal to ours, and one with no validator or a different one means the file changed (restart once, as above). While `If-Range` is still sent, a 206 without a validator is accepted, because the server has already checked it.
- **Giving up:** a connection that runs out of retries on a retryable error retires while other connections are still working: its unfinished range goes back to the queue and another connection picks it up. So a server that accepts fewer connections than we open (and refuses the rest with `503`) still completes the download over the ones it accepts. Only when the last active connection gives up does the download fail. Errors that are not retryable fail the download at once. Without range support a retry starts again from byte 0.
- **Range map:** a text file `<file>.ksdm` next to the download records the URL, size, validator and the finished byte ranges. It is written only when the server gave a strong validator, since without one the download can never be resumed safely. It is saved every 2 s and when the download stops, by syncing the download file first and then replacing the map atomically (write a temp file, sync, rename, sync the folder; `write_atomic` from ks-utility). All map writes and removals go through one lock, so a write left running by an aborted download can never collide with the next one or bring back a removed map, so it never lists bytes that are not on disk. A damaged map is ignored. On a fresh start the old map is deleted before the file is created or truncated, so a crash in between cannot leave a map describing bytes the file no longer has. The map is deleted when the download completes.
- **Pause and failure:** cancelling is a pause: connections write out what they buffered, the map is saved, and the partial file stays. The same happens when the last connection runs out of retries, on a file error, and on an internal error. Errors that make the partial file worthless (range ignored, wrong `Content-Range`, file changed) remove it, and so does stopping a download that has no strong validator. Deleting a paused download is an explicit call.
- **Progress:** `Progress::snapshot()` gives the total, the bytes received (overall and per connection) and each connection's current range (`start`, `position`, `end`; the end moves down when another connection takes the upper part). The caller polls it; the engine never sends an event per chunk. `ProbeInfo::resumable()` says up front whether a download can be paused and resumed.
- **Resume:** a new download to the same file resumes only if the map's URL, size and strong validator (an `ETag` that is not weak, or `Last-Modified`) match a fresh probe and the file has the expected size; every range request then carries `If-Range`. Otherwise it starts fresh. If a range request gets the whole file (200) with a different validator during the download, the engine probes again and starts fresh once; a second change fails the download.

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
