---
type: Plan
title: Build stages
description: The build stages, what each delivers, and the checks that must pass before a stage is committed.
tags: [plan, stages, process]
---

# Build stages

The app is built one stage per request. Stage 5 is built in one go, part by part, with a local commit after each part. Each stage ends with a build that runs, the checks in **Done means**, a test by the owner, and a commit only after the owner approves.

## Stage 1 — Scaffold and empty window
- Tauri 2 + React + TypeScript + Vite, Tailwind CSS, shadcn/ui, Lucide, on macOS (Windows build checked in the Release stage).
- Bundle identifier `com.ks.downloadmanager`; version comes from the git tag, never hardcoded.
- The main window from [ui](ui.md) with static sample rows: toolbar, sidebar, download list, status bar, light and dark theme following the system.
- Clear all toolbar action with a confirmation dialog (list only; files on disk are kept).
- Exact versions recorded in `CLAUDE.md`; every dependency in `docs/THIRD_PARTY.md`.

## Stage 2 — Engine core (single file)
- Rust crate `engine` inside `shared/src-tauri/`: probe (size, range support, `ETag`/`Last-Modified`), segmented range download into a pre-allocated file, buffered writes.
- Local test server that throttles each connection; unit and integration tests.

## Stage 3 — Dynamic splitting, retry, resume
- Work stealing between connections, per-range retry with backoff and jitter, `Retry-After`, crash-safe range map and `If-Range` resume.

## Stage 4 — UI wired to the engine
- Add download dialog, live list fed by batched progress events from Rust, pause / resume / delete, open file / folder, per-connection progress view.
- Downloads persist across restarts.

## Stage 5 — Final
Merged by the owner on 2026-11-21 from the open Stages 5–10 (scheduler, benchmarks, video and playlists, polish), plus the Chrome extension. Built part by part, in order, without stopping between parts. After each part: the **Done means** checks, `graphify update .`, and a local commit (never pushed). Open decisions are taken by the closest reading of the specs and recorded in `docs/decisions-stage5.md`.

Rules for the whole stage:
- [Free Download Manager](https://www.freedownloadmanager.org/) is the reference for feature scope and overall feel; [clean-room](clean-room.md) is unchanged (see [ui](ui.md#references)).
- Third-party dependencies stay at the minimum. Pre-approved: the yt-dlp, ffmpeg/ffprobe and Deno binaries, tokio features, and Zustand and TanStack Table/Virtual if justified in the decisions log. Anything else is written in `ks-utility` or left undone and listed.
- No hard-coded interface text: every string is looked up by key (Part 4).
- Out of scope: torrents.

### Part 1 — Single-video downloads
- yt-dlp as an unmodified sidecar, an LGPL ffmpeg/ffprobe we build (arm64 and x86_64, network protocols through the system TLS library, zlib kept), and Deno, per [video-playlists](video-playlists.md). Licences and third-party notices recorded and shipped. No DRM bypass.
- A video-site link in Add download is downloaded at the best available quality: one Download action, no quality picker, the video title as the file name, progress in the list like any other download. MP4 when the codecs fit, otherwise MKV, never re-encoded; a `.mkv` result says so in its row. Generic extractor off.
- A link that returns a web page and is not a supported video site shows "This link is a web page, not a file" and offers no way to save it.
- yt-dlp writes to a fixed name in a hidden temp folder beside the destination; on exit the app reserves `title.<real ext>` and renames. Arguments: `--ignore-config`, `--no-plugin-dirs`, `--no-remote-components`, `--ffmpeg-location`, `--js-runtimes deno:<path>`, a cache folder in app data, URL after `--`. Pause and cancel kill the whole process group.
- Extractor failures map to "the video component is out of date"; a manual "Update video component" action.
- Ad-hoc signed hardened-runtime build with a real download from it; what still needs a real certificate recorded.
- README: the app now bundles runtimes and is larger.

### Part 2 — Chrome extension (`shared/extension/`, TypeScript, Manifest V3)
- Native messaging restricted to our extension ID. The socket bind is the single-instance lock (stale socket: connect first, unlink and rebind only if that fails). Host mode checks its origin argument exactly. The app is started through LaunchServices. Socket path under the 104-byte limit. Host manifest written only if Chrome's folder exists.
- YouTube: one Download button on watch pages, Shorts and youtu.be links; one click, best quality, no menu.
- X (x.com, twitter.com): hover button for the largest version of an image. Other sites: the same button, off by default, enabled per site through optional host permissions.
- Optional hand-off of ordinary browser downloads, off by default.
- Only URLs are handed over: no cookies, no page content. Minimum permissions. Site rules in one small module. Loaded unpacked; install steps in the README.

### Part 3 — Scheduler, queues, categories, settings
- Global caps on total connections and active downloads; optional speed limit; queued downloads wait their turn.
- Adaptive connection count that can add connections back after a brief network problem, with a per-host limit remembered for the session.
- Categories with save folders, queues in the sidebar, the settings screen, the folder picker, a "prefer MP4" setting.

### Part 4 — Hindi alongside English
- All interface text in `en` and `hi` language files, looked up by key through our own translation helper in `ks-utility` (plurals, placeholders). No translation library.
- Rust sends message codes with parameters, not finished sentences.
- Language follows the system, with a remembered English/Hindi setting. Sizes, numbers and dates through built-in `Intl`.
- Extension labels in both languages through Chrome's `_locales`.
- A test fails when a key exists in one language file and not the other. Full Hindi text in `docs/hindi-review.md` for the owner's review.

### Part 5 — Playlists
- The playlist picker per [video-playlists](video-playlists.md), on the same sidecars.

### Part 6 — Benchmarks
- Repeatable benchmark against the throttled server (1, 4, 8, 16, 20, 32 connections), results in `docs/benchmarks.md`, a regression check ready for CI. No speed claim in the README or the app unless the numbers support it.

### Part 7 — Polish
- First-run notice (user's responsibility, no DRM bypass), full keyboard reachability, contrast, reduce-motion, the 10,000-row scroll test with a virtualised list, window material, clipboard pre-fill in Add download.
- Start-up restore of the download list off the critical path (see [ui](ui.md#deferred-from-stage-4)).
- Mark downloaded files as coming from the internet: the quarantine attribute on macOS; the Windows Mark of the Web written but marked untested until the Release stage.

### After Part 7 — Full testing
- Every check and test, the benchmark, and real downloads in the running app (direct file with pause and resume, a YouTube video, an X image through the extension, a playlist). Then `stage5-final-review.md` in the repository root for an outside review.

## Release
Plan: `docs/release-plan.md`; decisions: `docs/decisions-release.md`; owner's steps: `docs/release.md`.
- **Phase A (built, not yet run on GitHub):** versions from the tag (`shared/scripts/set-version.mjs`, only `vX.Y.Z`); CI on `macos-15` (`ci.yml`: build, every test, clippy, benchmark); a tag on `main` builds a **draft** release (`release.yml`): two macOS disk images (arm64, x64) made after signing, ad hoc unless all six Apple secrets exist (then Developer ID, notarised and stapled), the extension zip with its ID checked, the ffmpeg source, `SHA256SUMS`, and a build-provenance attestation for each file. The draft is published by hand only after the hand checks pass on its files. The Chrome host is not registered from a disk image or a translocated copy. Deno's notices give the standard text of each declared licence for crates without a licence file.
- **Phase B (built):** Windows on the same workflows: sidecars (ffmpeg cross-built on Linux), named-pipe hand-off with a same-user check, Chrome registration from the installer and at each start, no console windows, UTF-8 PowerShell, and tests for each Windows path plus a silent install/uninstall test, all on `windows-2025`. Shipped as an unsigned beta, untested by hand, starting with v0.1.2 (v0.1.0 and v0.1.1 were tagged but never released). Decisions: `docs/decisions-release.md`.

## Done means
- `npm run build` and `cargo test` (in `shared/src-tauri/`) succeed; `npm run tauri dev` opens the window.
- No new warnings from `cargo clippy`.
- New dependencies recorded in `docs/THIRD_PARTY.md` in the same commit.
- Owner has run the app and approved before anything is committed.
