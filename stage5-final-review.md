# Review request: Stage 5 (Final) of ks-download-manager, as built

You are reviewing work done by another Claude session (Claude Code) on a desktop app. **Review only: do not edit, create or delete any file in this folder or its neighbours, do not commit, push or run builds, and install nothing.** Read the code and docs, then write your review as your reply.

## Context
- This folder is `ks-download-manager`: a free desktop download manager. Rust (engine, file and network work) + Tauri 2 (shell); TypeScript + React + Tailwind + shadcn/ui (interface); a Chrome extension (TypeScript, Manifest V3) in `extension/`. macOS now; Windows code is written but untested until the Release stage.
- Stages 1–4 were done before. **Stage 5 (Final)** merged the old Stages 5–10 into Parts 1–7: single videos, Chrome extension, scheduler/queues/settings, Hindi, playlists, benchmarks, polish. It was built part by part on branch `dev`, with local commits only. Parts 1–4 were reviewed and approved by the owner along the way; Parts 5–7 were built in one go after the Part 4 review, without stopping.
- Generic code lives in a sibling repository, `../ks-utility` (pinned by tag, here `v0.2.3`). The workspace repository is the parent folder `..`.
- **Every open decision** taken while building is in `docs/decisions-stage5.md`, with its reason and the alternative not taken; items left undone are marked **Not done** there. Read it in full; it is the main companion to this file. The run rules the builder followed are in `docs/stage5-brief.md`.

## Read these first
- `CLAUDE.md` (stack, constraints, clean room, dependencies, shared code), `docs/stage5-brief.md`, `docs/decisions-stage5.md`
- Specs: `specs/stages.md` (Stage 5), `specs/video-playlists.md`, `specs/engine.md`, `specs/extension.md`, `specs/ui.md`, `specs/clean-room.md`, `specs/dependencies.md`
- `docs/THIRD_PARTY.md`, `docs/benchmarks.md`, `docs/hindi-review.md` (generated)
- Never read: `node_modules/`, `dist/`, `extension/dist/`, `src-tauri/target/`, `src-tauri/gen/`, `src-tauri/binaries/`, `package-lock.json`, `Cargo.lock`, `graphify-out/`. Ignore `stage5-plan-review.md` (an older review request kept as a style example).

---

# 1. What was built: commits per part

All on `dev`, local, not pushed. Each "Part N is done" section in the decisions log lists that part's checks.

| Part | Commit | What it does |
|---|---|---|
| Setup | `3209888` | Merges the open Stages 5–10 into Stage 5 (Final); Free Download Manager as the scope reference; clean-room note for extension site rules |
| 1 · Single videos | `42dca38` | yt-dlp (unpacked build), an LGPL ffmpeg we build, Deno; hidden temp folder beside the destination; final name reserved after yt-dlp exits; process-group stop; Update video component; ad-hoc hardened-runtime signing (`scripts/sign-macos.sh`) |
| | `3fddd99` | ks-utility pinned by tag `v0.1.0` |
| 2 · Chrome extension | `cd2721f` | Extension in `extension/`; the app as its own native-messaging host, restricted to one extension ID; socket as single-instance lock; engine fix for servers that ignore `If-Range`; file extension from Content-Type |
| 1 review | `1c8e5cf` | Temp-folder ownership marker, IDs never reused, GPG checks recorded, Deno licence texts, message fixes |
| | `ada337d`, `1b3e964` | The run brief |
| 2 review | `c1afa7c` | Validators required after dropping If-Range; dangerous Chrome downloads never taken over; private `/tmp` socket folder; silent connections closed; image look-up once per frame |
| Layout | `2ded514`, `ecbc50b` | Empty platform folders; `shared/` documented (no code moved in Stage 5) |
| 3 · Scheduler | `ef4373a` | Engine: adaptive connection count, shared connection budget, speed limit |
| | `7a7b9af` | App core: Waiting status, queues, categories, `settings.json`, folder picker, leftover download files |
| | `4956ff8` | Settings, Queues and leftover-files screens; category folders in Add |
| 3 review | `bc67b18` | Ownership markers on reservations; per-item leftover removal |
| 4 · Hindi | `ef34dbf`, `77a1cfb` | Decisions; pin ks-utility `v0.2.0` (language support, message codes, token bucket, media types) |
| | `4ab4d0d` | English and Hindi language files, Rust message codes, language setting |
| | `b4c4908`, `059e4e0` | Pin `v0.2.1`; the language-file check wired in; `docs/hindi-review.md` |
| 4 review | `c62ba3c` | New run rules (Parts 5–7 in one go; ks-utility may be pushed) |
| | `a027c3e`, `b7a5abf` | Pin `v0.2.2`; the owner's Hindi wording (स्टेटस, टेंप फ़ोल्डर, सिस्टम की भाषा, …) |
| | `71dd836` | Flaky crash test made reliable (cause: `abort()` left blocking-thread map writes landing after the test read the map; fix: the doomed download runs in its own runtime, dropped before reading; 0/300 failures under load, was 2/120) |
| | `4ebf31b` | Brief: commands without `cd` |
| 5 · Playlists | `3e849dd` | Picker (select all with count, shift-click ranges, thumbnails through Rust, unavailable marked), one row per entry grouped under the playlist, Resume/Pause playlist, playlist folder and numbered names |
| | `435d441`, `e6b8d9f` | Pin `v0.2.3` (playlist terms); Hindi review regenerated |
| 6 · Benchmarks | `e036808` | `npm run bench` against the throttled server, 10 % regression check against a committed baseline, a real-world test, `docs/benchmarks.md`; README claim rewritten to the measured numbers |
| 7 · Polish | `47316ec` | Downloads marked as from the internet (quarantine; Windows Mark of the Web untested), first-run notice, clipboard pre-fill through Rust, windowed list, start-up restore after the window opens, Shift+F10 menu |
| Testing | `52e6ad5` | Real-network test: direct file with pause, resume and the mark |
| Review file | (the commit adding this file) | This file; `docs/THIRD_PARTY.md` tokio features corrected (`net` was missing) |
| Final review | `9bcc823`, `5472aac`, `51caee3`, `e6620eb`, `64345f6`, `9d79150` | The review's changes (approved with changes): dead-server handling, list sorting and scrolling, thumbnail fetches, benchmark checks, nits; see `docs/decisions-stage5.md`, "Final review" |

**ks-utility** (`../ks-utility`, `main`): `a1d12b9` docs (pin by tag); `6ea7fb4` `v0.2.0` language support, message codes, token bucket, media types; `830f22b` + `a5aa8e7` `v0.2.1` command from `node_modules`, app terms; `70e9f2b` `v0.2.2` owner's wording; `96741cd` `v0.2.3` playlist terms. `v0.2.2` and `v0.2.3` were pushed by the builder under the owner's rule (fast-forward only, new tags only; each push recorded in the decisions log). **Workspace repo** (`..`): `6e17e8b`, `18fde06`, `96022ed`, `0b37916` (docs only, local).

# 2. Tests and checks, with results

Run at the end of Stage 5 on an Apple M5 Mac (macOS 27.2.1), unless marked.

| Check | Result |
|---|---|
| `npm run build` (`tsc -b` + Vite) | passed |
| `npm run test:lang` (ks-utility's language-file check on `src/locales` and `extension/_locales`) | passed |
| `npm run test:ui` (interface helpers: picker selection, grouping, durations, image check, windowing) | 9/9 |
| `npm run test:extension`, `npm run build:extension` | 6/6, built |
| `cargo test -q` in `src-tauri` (workspace) | 22 + 1 + 51 + 76 passed; 5 ignored (network or benchmark tests, run separately below) |
| `cargo clippy -q --all-targets -- -D warnings` | no output |
| ks-utility: `cargo test` (Rust) / `cargo clippy` / `npm test` (TypeScript: translator, check, Android conversion script and its command line, style-guide table against the term data) | 33/33 / clean / 17/17 |
| `npm run bench` (check mode against the baseline) | passed: 1, 4, 8, 16, 20, 32 connections → 1.00, 4.00, 8.00, 15.97, 19.95, 31.78× |
| Real-world benchmark (`real_world_single_against_engine`) | 1.06× on `proof.ovh.net` (the line was full with one connection); `speedtest.tele2.net` unreachable |
| Flaky crash test under load (12 copies at once) | 0/300 failures after the fix (2/120 before) |
| **Real downloads** (ignored tests, bundled yt-dlp, Deno and ffmpeg) | YouTube video: paused, resumed, finished (`real_video_download_pause_resume_and_finish`). Playlist: looked up, thumbnail fetched (WebP), one entry downloaded into `jawed - Videos/01 - Me at the zoo.mp4`. Direct file (100 MiB, Hetzner): paused at 720 KB, resumed, finished at full size, carries the quarantine attribute |
| **Extension hand-off, end to end** (debug app with a scratch `HOME`, Chrome-style framed messages to the app's host mode) | image (Wikimedia PNG), YouTube video and browser download (PDF): accepted, finished, each file carries `com.apple.quarantine` = `0081;…;KS Download Manager;`. A playlist link: refused with `playlist`. App restarted: list restored |
| App smoke test (debug build, scratch `HOME`) | starts, runs, restores and saves its list without an error |
| Contrast (computed from the theme's OKLCH colours, sRGB blending) | every text/background pair WCAG AA in both themes; lowest 4.94:1 |

## Not tested, and why
1. **Anything that needs eyes on the window:** the Add dialog with the playlist picker, the grouped list and its heading row, the first-run notice, Settings/Queues/leftover screens, the Hindi interface, keyboard reach (Tab order, Shift+F10), light/dark rendering. The building session cannot see or drive the window.
2. **60 fps with 10,000 rows:** the windowing maths is tested (at most 37 rows drawn at any position); frame rate needs the running window and a frame meter.
3. **Whether macOS 27 shows its "allow paste" prompt** when the app reads the clipboard through `pbpaste`.
4. **The extension inside Chrome** (buttons on YouTube and X, the popup, the browser-downloads switch): `chrome://extensions` cannot be driven from this session. The hand-off was tested by sending Chrome's exact frames instead. **No X image** was tested: no known public X image link was at hand without browsing X; a Wikimedia image was used for the image path.
5. **Windows:** process-group flags, `taskkill`, `attrib +h`, `certutil`/`tar`, the Mark of the Web, PowerShell folder picker and clipboard: written, never run (Release stage).
6. **Developer ID signing and notarisation:** only ad-hoc signing exists (list of what to check with a real certificate: decisions log, Part 1).
7. **Window material:** not built (see deviations).
8. **A long real playlist with private or deleted entries,** and a channel page whose tabs are nested lists: parsing tested with yt-dlp's output in unit tests only (yt-dlp sent the one channel tried straight to its Videos tab).
9. **Observed, not fixed:** when `proof.ovh.net` stopped answering mid-download (it seemed to block this address after several 100 MB downloads), the row stayed "Downloading" for more than 5 minutes before the test gave up. Retries and timeouts did not fail it within that time. Judge whether that is acceptable.

# 3. Deviations from the specs or the owner's prompts
1. **No quality picker and no audio-only choice for playlists** (spec offers both); the owner's single-video rule ("one Download action, no quality picker") was applied to playlists too.
2. **Benchmark file 64 MiB, not 1 GB** (same 1 MB/s-per-connection rate; the 1 GB single-connection run would take 17 minutes). Adaptive connections off in the benchmark, as the owner allowed. No CI workflow yet; the check is a command ready for one.
3. **README speed claim:** "up to 20×" removed; it now quotes 19.95× locally with its condition and 1.06× in the real world.
4. **Window material: Not done** (needs Tauri's `macos-private-api` for a transparent window; contrast not checkable here).
5. **ks-utility tag `v0.2.1` was moved once** before it was ever pushed (decisions log, Part 4). The rule forbids moving pushed tags.
6. **Number prefix on by default** for playlist files (spec: optional).
7. **Graph tooling:** a full `/graphify` rebuild stopped at its shrink guard (63 old semantic nodes); the existing graph was kept. Development tool only, not part of the app.
8. Smaller ones are in the decisions log, each with its alternative (for example: `.ksowner` for every engine download, defaults 3/64/16, extension replies carry codes only, unavailable playlist entries never added, thumbnails capped at 1 MB).

# 4. New dependencies and binaries in Stage 5

| Item | Licence | How it is verified |
|---|---|---|
| yt-dlp 2026.10.26 (unpacked macOS build) | Unlicense; bundled Python libraries listed in its `THIRD_PARTY_LICENSES.txt` (shipped) | SHA-256 pinned in `src-tauri/sidecars.json` from the release's `SHA2-256SUMS`, whose GPG signature was checked; `scripts/sidecars.mjs` refuses a mismatch. In-app update: hash from the same release, checked; not marked as quarantined (decided, `component.rs`) |
| ffmpeg/ffprobe 9.1, built by `scripts/build-ffmpeg.sh` | LGPL-2.1-or-later (no GPL, no nonfree, autodetect off; zlib and Secure Transport only) | Source tarball SHA-256 pinned, `.asc` GPG signature checked; licence texts and configure line shipped in `licenses/` |
| Deno 2.9.8 | MIT; `licenses/deno-THIRD-PARTY.txt` (8.5 MB; 123 crates without a licence file are listed by declared licence only) | SHA-256 pinned from the release's `.sha256sum` (Deno publishes no signature) |
| tokio features `process`, `net` (app), already in the lock file | MIT | — |
| ks-utility `v0.2.0`–`v0.2.3` | MIT (first-party, no third-party code) | Tag pinned in `package.json` and both `Cargo.toml`; both lock files name `96741cd` |
| GnuPG (Homebrew) | development tool only, not shipped | — |

No other crate or npm package was added. Zustand and TanStack were not added (reasons in the decisions log and `specs/ui.md`).

# 5. Security-sensitive areas: look here first
1. **Native-messaging hand-off** (`src-tauri/src/handoff.rs`, `extension/src/`): host-mode origin check (exact extension ID), message parsing (unknown fields, size caps, URL rules), the Unix socket and its `/tmp/ksdm-<uid>` fallback (must be a real `0700` folder owned by us), the 5 s line timeout, single-instance handling, the kind check (image/file/video/playlist), dangerous Chrome downloads never handed over.
2. **Running yt-dlp, ffmpeg and Deno** (`src-tauri/src/video.rs`): fixed arguments (`--ignore-config`, `--no-plugin-dirs`, `--no-remote-components`, no cookies, generic extractor off, `--flat-playlist`), the URL after `--`, process-group stop, the output file accepted only inside our temp folder, the in-app yt-dlp update (`component.rs`).
3. **File naming and deletion:** reservations and `.ksowner` markers, `complete`/`complete_video` (never overwrite), `video_temp` and `remove_video_temp`, the playlist folder (`playlist_folder`: one level, refuses files and links), **the leftover-files cleanup** (`leftovers.rs`: known folders only, links not followed, marker re-checked before each removal, only ticked items).
4. **New in Parts 5–7:** playlist picks checked against the looked-up list (`add_playlist`); thumbnail fetch (`Manager::thumbnail`: kept playlists only, https only, 1 MB cap, image signature check); the clipboard read (`clipboard.rs`: only one http(s) link is passed on); internet marking (`mark.rs`, called before the final rename); the restore gate (`restored`, nothing scheduled before it).
5. **The extension's permissions** (`extension/manifest.json`): `nativeMessaging`, `scripting`, `contextMenus`, `activeTab`, `storage`; `downloads` only as an optional permission, asked for when the browser-downloads switch is turned on; host permissions for YouTube, X and Twitter only; every other site opt-in through optional host permissions.

# 6. What the owner must test by hand
1. Open the app (`npm run tauri dev`): the first-run notice appears once; **I understand** closes it; it does not come back after a restart.
2. Copy a file link, press Ctrl/Cmd+N: the link is already in the field and selected. Note whether macOS asks to allow pasting. Copy plain text: the field stays empty.
3. Paste a YouTube playlist link: the picker lists title, duration and thumbnails; all available entries are ticked; Select all clears and re-ticks with the right count; shift-click sets a range; unavailable entries are marked and cannot be ticked.
4. Download a few entries: one folder named after the playlist, files "01 - …", rows grouped under the playlist heading; Pause playlist and Resume playlist work; fold the heading; a failed entry can be retried with Resume playlist.
5. Finder → Get Info on a finished file: it says it was downloaded from the internet; opening a downloaded app or script triggers Gatekeeper's check.
6. Keyboard only: Tab through toolbar, sidebar and list; arrow keys; Space pause/resume; Delete; Shift+F10 opens the row menu; every dialog can be completed and closed.
7. Switch to Hindi in Settings: no English left on any screen (including the playlist picker and the notice); check the wording in `docs/hindi-review.md` (17 strings marked ⚠, 5 new terms marked for review).
8. Light and dark mode: readable everywhere, including the selected row and the playlist heading. Turn on Reduce motion: no moving spinners or bars.
9. Fill the list with many rows (for example, add a long playlist several times) and scroll quickly: smooth, no blank rows.
10. Load the extension unpacked (README): the YouTube button, the X hover button on an image (largest version saved, marked), the popup, the browser-downloads switch.
11. Start the app with many unfinished downloads: the window appears at once, the list fills a moment later, nothing starts by itself.
12. Settings: limits, speed limit with a video, Prefer MP4, Update video component, leftover download files (Remove only after confirming).
13. *(After the final review.)* Start a large download and pull the network cable (or block the server): after about 15 s the row says "Server not answering, retrying…"; after about 2 minutes it fails with "The server stopped answering…"; reconnect and Resume continues from where it stopped. While other rows download, the list does not jump back to the selected row.
14. *(After the final review.)* A long playlist (hundreds of entries) opens at once and scrolls smoothly in the picker, thumbnails appear as entries come into view. Settings → Add download → turn off **Fill in a link from the clipboard**: Add download stays empty with a link on the clipboard (and macOS shows no paste prompt).

# 7. Instructions for the reviewer
1. **Requirements coverage:** for every item of Stage 5 in `specs/stages.md` and `docs/stage5-brief.md` (Parts 1–7 and the final steps), say whether it is met, half met or missing. Check that every **Not done** in the decisions log is honest and nothing undone is presented as done.
2. **Rule conflicts:** `CLAUDE.md` and the specs: clean room, minimum dependencies, network only for downloads the user starts (judge the thumbnail fetch and the clipboard read against this), no analytics or ads, no hard-coded interface text, both languages for all new text, signing material never committed, versions from tags, no code moved into `shared/` or platform folders, no Claude attribution in commit messages.
3. **Security:** go through section 5 in order. Can a web page, another extension or another local program add or start downloads, or make the app write, overwrite or delete a file it does not own? Can a crafted playlist (titles, URLs, positions), thumbnail reply or clipboard text do anything beyond what is described?
4. **Correctness:** the restore gate (anything that can start before it, or be changed by it); the windowed list (selection, scrolling, folded groups, `aria-*`); marking before the final rename (any path that skips it, or marks the user's own file); the flaky-test fix (does it still test a real crash?); playlist numbering and naming; the benchmark's regression check (could it pass while the engine got slower?).
5. **Deviations (section 3):** agree or disagree with each, with a reason.
6. **What remains:** anything that must happen before the Release stage, beyond the list in the brief ("Later, not in Stage 5").

# Reply format
- Start with a one-line verdict: **Approve**, **Approve with changes** or **Rework**.
- Then findings, most serious first. Each: severity (**blocker** / **should fix** / **nit**), file and line or section, the problem, and a concrete fix.
- Then a short list: requirements not met or only half met.
- Then your view on each deviation in section 3.
- Say which claims you checked against the code and which you took from the docs.
- Keep it under about 1,200 words. No rewrite of the code or the docs.
