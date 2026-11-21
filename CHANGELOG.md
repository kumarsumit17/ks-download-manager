# Changelog

All notable changes to KS Download Manager. The layout follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/) and come from git tags.

## [Unreleased]

## [0.1.1] - 2026-11-21

v0.1.0 was tagged but never released: its tag points to a commit whose changelog section was not yet dated, so the release workflow stopped before building anything. The tag is kept as it is; this is the same content, released as 0.1.1.

The first public release: macOS (Apple silicon and Intel) and the Chrome extension, plus a Windows installer as a **beta, untested by hand** (its automated tests pass on Windows Server 2025 in CI; no person has run it on a Windows PC yet).

### Added

- Downloads over many connections at once, with connections splitting the remaining work between them as they finish, per-range retry with backoff, and resume after a pause, a crash or a restart when the server allows it. On a local test server that limits each connection, 20 connections were 19.95× as fast as one; real-world gains are smaller when one connection already fills the line (see `docs/benchmarks.md`).
- One window with a toolbar, categories and queues in the sidebar, and the download list; light and dark themes follow the system.
- Add download dialog with clipboard pre-fill, pause, resume, delete, open the file or its folder, and a view of each connection's progress. Downloads are kept across restarts.
- Single videos from YouTube and other sites supported by yt-dlp, at the best available quality (MP4 when the formats allow, otherwise MKV, never re-encoded), and whole playlists through a picker. Bundled, unmodified yt-dlp, Deno and an LGPL build of ffmpeg; an "Update video component" action. No DRM bypass.
- Chrome extension (loaded unpacked): a Download button on YouTube watch pages and Shorts, a button for the largest version of an image on X and on other sites you enable, and optional hand-off of ordinary browser downloads. It sends links only, never cookies or page content.
- Limits on total connections and active downloads, an optional speed limit, queues, categories with their own save folders, a settings screen and a "prefer MP4" setting.
- English and Hindi throughout the app and the extension; the language follows the system and can be chosen in settings.
- Downloaded files are marked as coming from the internet (the quarantine attribute on macOS, the Mark of the Web on Windows).
- Windows 10 and 11 installer, **beta, untested by hand**: per-user, not code-signed, registers the app with Chrome and removes that registration when uninstalled.
- First-run notice: you are responsible for having the right to download what you request.
