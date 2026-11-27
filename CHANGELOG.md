# Changelog

All notable changes to KS Download Manager. The layout follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/) and come from git tags.

## [Unreleased]

## [0.4.1] - 2026-11-27

v0.4.0 was tagged but never released: its release stopped at the first check, because ks-utility had a newer version. This is the same release on ks-utility v0.18.4.

### Changed

- Installing an update on macOS: once the disk image is open, the app says it now closes and quits after a few seconds, so dragging the new version into Applications replaces it straight away.

## [0.3.0] - 2026-11-27

### Added

- The browser extension now works in Microsoft Edge too: the app registers itself with Edge as well as Chrome (on Windows and macOS).
- On Windows, the taskbar button shows how many downloads are running, turns yellow while the servers of all running downloads have stopped answering, and red when a download fails, until you look at the window.
- On Windows 11, the title bar uses the Mica material.

### Changed

- Windows contrast themes: progress bars, the selected download, keyboard focus and buttons stay visible.

## [0.2.0] - 2026-11-26

### Added

- While downloads run, the computer stays awake (a new switch in Settings, on by default), and the Dock icon or taskbar button shows their progress; on macOS the Dock badge shows how many are running.
- A notification when a download finishes or fails while the window is in the background (a new switch in Settings, on by default).
- Drag a link, or a `.webloc` or `.url` file, onto the window to add it as a download.
- On macOS, a full menu bar (File, Edit, Downloads, View, Window, Help) with the usual shortcuts, such as ⌘N to add a download and ⌘1–⌘7 for the lists.
- On macOS, finished files record the link they came from (Finder's Get Info and Spotlight show it), and files saved in Downloads make the Dock's Downloads stack bounce.

### Changed

- The project moved to the GitHub account `kumasumit` (https://github.com/kumasumit/ks-download-manager). Copies installed before this release cannot find it by themselves: download it once from the Releases page; later updates arrive as usual.
- The code is now laid out per platform: the shared Tauri project in `shared/`, each operating system's own code in `desktop/mac` and `desktop/windows`.
- macOS, Windows and the Chrome extension can now be released on their own, each with its own version (tags `mac-v…`, `windows-v…`, `chrome-v…`). The daily update check looks for the newest release for this computer's system, and the extension is told the newest extension version.
- The files are now attested by KS-Suite's shared release workflow: check them with `gh attestation verify <file> -R kumasumit/ks-download-manager --signer-workflow kumasumit/ks-utility/.github/workflows/tauri-app-release-publish.yml`.

## [0.1.5] - 2026-11-23

The update check now uses the KS-Suite shared update code; nothing to do by hand. 0.1.4 finds this version by itself.

### Changed

- While the app stays open, it asks again every hour whether the daily update check is due, so a copy left running for days still checks once a day.
- The downloaded installer's SHA-256 is computed by the app itself instead of by the system's `shasum` or `certutil`.
- The App updates texts come from the shared KS-Suite catalogue, in English and Hindi.

## [0.1.4] - 2026-11-23

App updates and IDM-style hand-off of Chrome downloads. Earlier copies cannot update themselves: install 0.1.4 by hand once, from the disk image or the Windows installer (still a beta, untested by hand), and load the new extension zip in Chrome (it now asks to read and change data on all websites, so it can send a download's cookies). The 0.1.4 app and extension work only together: an older app refuses downloads from the new extension, which then stay in Chrome.

### Added

- App updates: the app checks GitHub once a day for a newer version (a switch in Settings turns this off) and has a **Check for updates** button. When one is found, it downloads the installer for your system, checks its SHA-256 and opens it: on macOS you drag the new app into Applications, on Windows the installer starts and the app closes.
- The Chrome extension's popup says when a newer version is out, with a link to the release.
- Downloads you start in Chrome go to the app, on by default, like IDM: you track, pause and resume them there. With each one go that site's cookies for the file, the page it came from and Chrome's identification, so downloads that need your sign-in work too; the app uses them only for that download, sends the cookies only to that site and never saves them. The popup shows what happened to the last download.

### Fixed

- Sending browser downloads to the app never worked: the extension waited for a safety rating Chrome rarely gives at that point, and the switch was not saved when Chrome's permission prompt closed the popup.

## [0.1.3] - 2026-11-23

The same app as 0.1.2; this release completes its files.

### Changed

- The release includes the exact ffmpeg source the bundled ffmpeg and ffprobe are built from (`ffmpeg-9.1.tar.xz`, LGPL-2.1-or-later), next to the two macOS disk images, the Windows installer (beta, untested by hand) and the Chrome extension. 0.1.2 was published without it.
- `licenses/ffmpeg-SOURCE.txt` in the app says the source tarball is attached to each release.
- Releases are published by the release workflow directly from the version tag, once every build and check has passed.

## [0.1.2] - 2026-11-22

v0.1.0 and v0.1.1 were tagged but never released; both tags are kept as they are, and this is the same app, released as 0.1.2. v0.1.0 pointed to a commit whose changelog section was not yet dated, so the release workflow stopped before building anything. For v0.1.1 the workflow failed to package the Chrome extension (an environment variable named `ZIP`, which `zip` reads as its own options), so no draft was made.

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
