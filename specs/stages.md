---
type: Plan
title: Build stages
description: The nine build stages, what each delivers, and the checks that must pass before a stage is committed.
tags: [plan, stages, process]
---

# Build stages

The app is built one stage per request. Each stage ends with a build that runs, the checks in **Done means**, a test by the owner, and a commit only after the owner approves.

## Stage 1 — Scaffold and empty window
- Tauri 2 + React + TypeScript + Vite, Tailwind CSS, shadcn/ui, Lucide, on macOS (Windows build checked in Stage 9).
- Bundle identifier `com.ks.downloadmanager`; version comes from the git tag, never hardcoded.
- The main window from [ui](ui.md) with static sample rows: toolbar, sidebar, download list, status bar, light and dark theme following the system.
- Clear all toolbar action with a confirmation dialog (list only; files on disk are kept).
- Exact versions recorded in `CLAUDE.md`; every dependency in `docs/THIRD_PARTY.md`.

## Stage 2 — Engine core (single file)
- Rust crate `engine` inside `src-tauri/`: probe (size, range support, `ETag`/`Last-Modified`), segmented range download into a pre-allocated file, buffered writes.
- Local test server that throttles each connection; unit and integration tests.

## Stage 3 — Dynamic splitting, retry, resume
- Work stealing between connections, per-range retry with backoff and jitter, `Retry-After`, crash-safe range map and `If-Range` resume.

## Stage 4 — UI wired to the engine
- Add download dialog, live list fed by batched progress events from Rust, pause / resume / delete, open file / folder, per-connection progress view.
- Downloads persist across restarts.

## Stage 5 — Scheduler, queues, categories
- Global caps on connections and active downloads, speed limit, adaptive connection count, per-host limit memory.
- Categories by file type with save folders; queues in the sidebar; settings screen.

## Stage 6 — Benchmarks
- Repeatable benchmark against the throttled server (1, 4, 8, 16, 20, 32 connections), results in `docs/benchmarks.md`, regression check in CI. No speed claim before this stage passes.

## Stage 7 — Video and playlists
- yt-dlp and ffmpeg as sidecars, licences recorded; single video and playlist picker per [video-playlists](video-playlists.md); yt-dlp self-update.

## Stage 8 — Polish
- First-run notice (user's responsibility, no DRM bypass), keyboard reachability, contrast, reduce-motion, 10,000-row scroll test, window material (Mica / vibrancy).
- Start-up restore of the download list off the critical path (see [ui](ui.md#deferred-from-stage-4)).
- Mark downloaded files as coming from the internet: the `com.apple.quarantine` attribute on macOS and the Mark of the Web (`Zone.Identifier` stream) on Windows, so the system warns before opening them. In Stage 8, or at the latest in Stage 9 with the Windows build.

## Stage 9 — Release
- Windows build, signing via `SIGNING_*` secrets in CI, tag-driven versions, release workflow.

## Later (not scheduled)
- Chrome extension that hands URLs to the app.

## Done means
- `npm run build` and `cargo test` (in `src-tauri/`) succeed; `npm run tauri dev` opens the window.
- No new warnings from `cargo clippy`.
- New dependencies recorded in `docs/THIRD_PARTY.md` in the same commit.
- Owner has run the app and approved before anything is committed.
