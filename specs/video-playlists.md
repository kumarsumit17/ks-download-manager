---
type: Specification
title: Video and playlist downloads
description: Single-video and playlist downloads through yt-dlp and ffmpeg, the best-available-quality option, and their limits.
tags: [video, playlists, quality]
---

# Video and playlist downloads

## What the user can do
- Paste a link to a single video or a whole playlist from YouTube or another supported video site.
- For a playlist: see the list of entries with title, duration and thumbnail, choose quality once for all, and start. Every entry is selected when the list opens, so downloading the whole playlist needs no ticking at all.
- A **Select all** box at the top of the list ticks or unticks every entry in one click, with a count beside it ("42 of 42 selected"). Individual boxes are only for leaving videos out. Shift-click selects a range.
- Choose format per download: video (resolution choice) or audio only.
- Or skip the choice: with **Always get the best available quality** on, the app picks the best version itself. See [Best available quality](#best-available-quality).
- Each playlist entry appears as its own row in the download list, grouped under the playlist name, saved to a folder named after the playlist, with an optional number prefix to keep the order.
- Pause, resume and retry work per entry and for the whole playlist. Entries that are unavailable (private, removed, region-blocked) are marked and skipped, without stopping the rest.

## Best available quality
One setting, **Always get the best available quality** (Settings → Downloads), on by default. It covers video, audio and images.

- **Video:** the highest resolution the site offers for that video, then the highest frame rate, then the highest bitrate, joined with the best audio stream. No resolution question is asked.
- **Audio only:** the highest-bitrate audio stream, kept in the format the site serves. It is not re-encoded, because converting cannot add quality and usually loses some.
- **Images:** when a site offers the same image in several sizes, the original or largest one, never a preview or thumbnail. A direct link to an image file downloads that file as it is.
- **Where it shows:** the Add download dialog and the playlist picker show "Best available" as the quality and start without asking. The user can still pick a lower quality there for one download or one playlist. With the setting off, the quality choice is asked every time.
- **Honest limits:** "best" means the best version the site gives to a visitor who is not signed in. Formats behind a sign-in, a paid tier or DRM are not included (see Limits). Best quality also means the largest files and the longest downloads.
- **If the best version fails** (removed or blocked part-way), the download is marked failed with the reason. The app does not quietly fall back to a lower quality.

## How it works
- Site support comes from **yt-dlp**, run as a separate bundled program (Tauri sidecar) that the Rust engine calls for two jobs: listing a link's entries and formats, and fetching the media. **ffmpeg** is used to join separate video and audio streams.
- Reason: every video site changes often and needs constant extractor updates. Writing and maintaining our own extractors for many sites is not realistic; yt-dlp is the maintained, openly licensed tool for this. It is used unmodified as a dependency, which is consistent with [clean-room](clean-room.md).
- The app can update yt-dlp on its own, separately from app releases, because sites break old versions quickly.
- Ordinary file downloads (images, archives, direct media links) do not go through yt-dlp; they use our own segmented Rust engine.

## Bundled programs (Stage 5)
| Program | What we ship | How it is checked | Licence |
|---|---|---|---|
| yt-dlp | The official unpacked macOS build (`yt-dlp_macos.zip`, universal), unmodified, in the app's resources (`yt-dlp/`) | Version and SHA-256 pinned in `src-tauri/sidecars.json`; `scripts/sidecars.mjs` refuses a file whose hash differs | Unlicense; its bundled Python and libraries are listed in `yt-dlp/_internal/THIRD_PARTY_LICENSES.txt`, shipped with it |
| ffmpeg, ffprobe | Built by us from the pinned ffmpeg.org source tarball by `scripts/build-ffmpeg.sh`, for arm64 and x86_64 | Tarball SHA-256 pinned; the build stops on a mismatch | LGPL-2.1-or-later: no `--enable-gpl`, no `--enable-nonfree`, `--disable-autodetect` with only zlib and Secure Transport (system libraries) enabled |
| Deno | The official release binary for the target architecture | Version and SHA-256 pinned | MIT; `licenses/deno-THIRD-PARTY.txt` lists the crates built into it |

- **Why the unpacked yt-dlp:** the single-file build unpacks Python and its libraries to a temp folder on every run, which is slow and whose libraries cannot be signed with the app. The unpacked build is signed once with the app.
- **ffmpeg network:** network protocols stay enabled, with TLS through the system's Secure Transport, so sites where yt-dlp hands an HLS stream to ffmpeg keep working. zlib is enabled explicitly (`--disable-autodetect` would otherwise drop it).
- **LGPL obligations:** the licence texts and the exact source reference and configure line are shipped in `licenses/` (`ffmpeg-SOURCE.txt`); the build script is in this repository; the same source tarball is attached to each release (Release stage). ffmpeg is a separate program the user can replace.
- **One architecture per build:** Deno alone is about 80–100 MB per architecture, so a universal app would roughly double in size. Each release is built for one architecture (`aarch64-apple-darwin` or `x86_64-apple-darwin`), with that architecture's ffmpeg and Deno.

## How a video download runs
- **Recognising a link:** our engine probes the link first. If it answers with a file, it is an ordinary download. If it answers with a web page (`text/html`), yt-dlp is asked (`-J`, 60 s limit) with the generic extractor off (`--ies default,-generic`), so only sites yt-dlp knows by name count. If none knows it, the Add dialog says "This link is a web page, not a file" and offers no way to save the page. Links that the engine gets 401, 403, 405 or 429 for are also tried with yt-dlp.
- **One action:** the Add dialog shows the title, "Best available quality" and the estimated size, and a single **Download** button. No quality question.
- **Arguments on every run:** `--ignore-config`, `--no-plugin-dirs`, `--no-remote-components`, `--no-update`, `--no-cookies-from-browser`, `--ffmpeg-location <app folder>`, `--js-runtimes deno:<bundled deno>`, `--cache-dir <app data>/yt-dlp-cache`, `--ies default,-generic`, `--no-playlist`, and the URL after `--`, so it can never be read as an option. No cookies are ever passed.
- **Format:** `-f "bv*+ba/b" -S res,fps,br --merge-output-format mp4/mkv`. MP4 when the codecs fit, otherwise MKV; never re-encoded. A finished `.mkv` says so in its row ("MKV · plays in VLC or IINA"). The "Prefer MP4" setting adds codec preferences after resolution and frame rate, so it never lowers the resolution.
- **Files:** yt-dlp writes to the fixed name `video.<ext>` (with its `.part` files) inside a hidden folder beside the destination, `.ksdownload-video-<id>/`, used for both its home and temp paths. The folder holds a marker, `ks-owner.json`, with the app identifier, the download ID, the URL and the creation time. An existing folder is continued in only when its marker names this download's ID and URL; with no marker or another URL it is never touched, and a fresh folder (`.ksdownload-video-<id>-<n>`, recorded with the download) is used instead; the old one is left for the leftover-files cleanup. Download IDs are never reused (the next ID is saved in `downloads.json`), so a leftover folder can only ever name its own download. The final name is not reserved before it runs (yt-dlp skips a download whose output exists, and the extension is only known after merging). When yt-dlp exits successfully, the app reserves `title.<real ext>` in the destination folder ("title (1).ext" if taken) and renames the file over the reservation, then removes the hidden folder. Nothing is ever overwritten.
- **Progress:** yt-dlp prints one machine-readable line per update (format, bytes, total or estimate); video and audio come one after the other and are added up. While ffmpeg joins them the row shows "Merging…".
- **Pause, delete, quit:** yt-dlp runs in its own process group; stopping sends SIGTERM to the whole group (yt-dlp, ffmpeg, Deno), then SIGKILL after 3 s. Resume runs yt-dlp again on the same folder; it continues its `.part` files and fetches fresh stream addresses. After a restart a video with its hidden folder comes back paused; without it, queued. Delete removes only that download's hidden folder.
- **Failures:** the last `ERROR:` line decides the message: web page, sign-in needed, a temporary block by the site ("confirm you're not a bot", too many requests: try later or on another network), unavailable, DRM (not supported), live stream (not supported), refused (HTTP 403: blocked in the region, a sign-in, or an old component; the update is offered), or extractor failures (unable to extract, signature or `nsig` failures, "please report this issue"), which say "the video component is out of date" and offer **Update video component**. A typed file name keeps its full text; only a known media extension (`.mp4`, `.mkv`, …) is replaced by the real one. There is no fallback to a lower quality.

## Updating yt-dlp
- **Update video component** (offered on a failed video, and in Settings) asks GitHub for the newest yt-dlp release, only when the user clicks it. If it is newer than the one in use, the app downloads `yt-dlp_macos.zip` and that release's `SHA2-256SUMS`, checks the hash, unpacks it into `<app data>/video-component/<version>/`, checks that it starts and reports that version, and only then switches to it (a `current` file names the version). The bundled copy inside the signed app is never changed; older updates are removed.
- **Updates and "downloaded from the internet" marking (decided for Part 7):** the app marks the files the user downloads with the quarantine attribute, so macOS checks them before they are opened. The yt-dlp update is not a user download and is **deliberately not marked**: a marked program that is not notarised by us (yt-dlp's build is not) would be refused by macOS, and the update would stop working. This is an explicit exemption in the code (`component.rs`), not an omission. Trade-off: Gatekeeper does not check the updated yt-dlp; it is trusted on the strength of HTTPS to GitHub and the hash from the same release, which proves an intact download but not that the release itself is genuine. The update runs only when the user clicks it, never in the background. Alternatives not taken: checking the release's GPG signature in the app (needs an OpenPGP library, not approved), or updating yt-dlp only through app releases (safer, but sites break old versions within days).
- **What the hash proves:** the hash comes from the same release as the file, so it detects a damaged or incomplete download, not a deliberately altered release. The pinned hashes in `sidecars.json` are taken from the release's `SHA2-256SUMS` when the pin is moved; checking those sums' GPG signature by hand when moving the pin is recommended (not done yet: see `docs/decisions-stage5.md`).

## Limits
- No DRM-protected content (paid streaming services). The app does not attempt to remove or bypass copy protection.
- Private or sign-in-only videos are out of scope for the first version.
- The user is responsible for having the right to download what they request. The first-run screen and the README say so plainly.

## Open points
- Best-size images: the extension's site rules cover X (x.com, twitter.com); other sites use the largest `srcset` candidate or a linked full-size image (Stage 5, Part 2).
- Chrome Web Store rules: not relevant, the extension is loaded unpacked and not published.
