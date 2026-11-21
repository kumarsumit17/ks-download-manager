# KS Download Manager

> Part of the [KS-Suite](https://github.com/kumarsumit17/KS-Suite).

A free download manager for images, audio and video, built for speed: each file is fetched over many connections at once. No ads and no tracking.

| | |
|---|---|
| **Platform** | Windows · macOS (desktop) |
| **Tech stack** | Rust (download engine) · Tauri (desktop shell) · TypeScript + React with shadcn/ui and Lucide icons (interface) |
| **Chrome extension** | TypeScript, Manifest V3, in `extension/` (loaded unpacked, not on the Chrome Web Store) |
| **Status** | First release (v0.1.2; v0.1.0 and v0.1.1 were tagged but never released): macOS and the Chrome extension; Windows as a beta, untested by hand. See [stages](specs/stages.md) and the [changelog](CHANGELOG.md) |

## Features

- Download images, audio and video
- Download single videos and whole playlists from YouTube and other video sites ([spec](specs/video-playlists.md))
- Faster downloads by fetching each file over many connections at once. On a local test server that limits each connection, 20 connections were 19.95× as fast as one. That shows how the engine scales, not real-world speed: when one connection already fills your line, more connections add little (1.06× in our one real-world test). Numbers and conditions: [docs/benchmarks.md](docs/benchmarks.md) ([how](specs/engine.md))
- Free, with no ads, analytics or tracking

## Interface

A clean, classic single-window layout: toolbar on top, categories and queues on the left, the download list in the middle, in light and dark themes. See [specs/ui.md](specs/ui.md).

## Install

Downloads are on the [Releases page](https://github.com/kumarsumit17/ks-download-manager/releases). **The app does not check for updates** (it uses the network only for the downloads you start); new versions are published on that page, so look there for updates.

### macOS (11 or later)

1. Download the disk image for your Mac: `…-macos-arm64.dmg` for Apple silicon (M1 and later), `…-macos-x64.dmg` for Intel.
2. Open it and drag **KS Download Manager** to **Applications**. Open the app from Applications, not from the disk image (from the disk image it cannot register with Chrome).
3. If the release notes say the build is **not signed with an Apple Developer ID**, macOS refuses the first open ("cannot be opened" or "damaged"). Try to open it once, then open **System Settings → Privacy & Security**, choose **Open Anyway** next to the message about KS Download Manager, and confirm. Or, in Terminal: `xattr -dr com.apple.quarantine "/Applications/KS Download Manager.app"`.

To remove it, drag the app to the Bin. It leaves Chrome's small registration file behind, which does nothing without the app; delete it by hand if you like: `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.ks.downloadmanager.json`. Settings and the download list are in `~/Library/Application Support/com.ks.downloadmanager`.

### Windows 10 and 11 (beta, untested by hand)

The Windows installer is a **beta**: its automated tests pass on Windows in CI, but no person has tried it on a Windows PC yet. Please report what does not work.

1. Download `…-windows-x64-setup-beta.exe`.
2. The installer is not code-signed, so SmartScreen says "Windows protected your PC": choose **More info → Run anyway**.
3. It installs for your user account only, without administrator rights, and registers the app with Chrome (if Chrome has been opened at least once on this PC). WebView2, which Windows 10 and 11 normally have, is installed for you if it is missing.

To remove it: **Settings → Apps → Installed apps → KS Download Manager → Uninstall**. That also removes the app's Chrome registration.

### Checking a download

Each release has `SHA256SUMS`, the SHA-256 of every file. On macOS, in the folder with the files: `shasum -a 256 -c SHA256SUMS --ignore-missing`. On Windows: `Get-FileHash <file>` and compare. Every file also has a build-provenance attestation showing it was built by this repository's GitHub Actions from the release tag; with the [GitHub CLI](https://cli.github.com): `gh attestation verify <file> -R kumarsumit17/ks-download-manager`.

## Building

```bash
npm install
npm run sidecars        # fetch yt-dlp and Deno, build ffmpeg (checked against pinned SHA-256)
npm run tauri dev       # run with hot reload
npm run bundle:mac      # release .app, then sign the bundled programs (ad-hoc unless an identity is given)
```

## Chrome extension

The extension adds a Download button on YouTube watch pages and Shorts, and on images on X (and on other sites you enable one by one). It only sends links to the desktop app; the app does the downloading. It never sends cookies or page content, so only media anyone can open without signing in works. Details: [specs/extension.md](specs/extension.md).

It is not published on the Chrome Web Store. To install it:

1. Download `ks-download-manager-extension-vX.Y.Z.zip` from the [Releases page](https://github.com/kumarsumit17/ks-download-manager/releases) and unzip it to a folder you keep (Chrome loads it from there, so do not delete or move that folder). Or build it yourself: `npm install`, then `npm run build:extension`, and use `extension/dist`.
2. Open the desktop app once (from Applications on macOS), so it registers itself with Chrome.
3. In Chrome, open `chrome://extensions`, turn on **Developer mode** (top right), click **Load unpacked** and choose that folder.
4. The extension's ID must be `ikplfdbdfikcokpegkapjdedlgifmkbe`; the app accepts no other.

If the app is removed, the extension says the app is not installed; remove the extension from `chrome://extensions`.

Chrome may show a notice about extensions in developer mode; that is expected for an unpacked extension.

## Clean-room implementation

This is an independent implementation: no code or assets from other download managers. See [specs/clean-room.md](specs/clean-room.md).

You are responsible for having the right to download what you request. The app does not bypass DRM or other copy protection.

## Why this stack

- **Fast:** the download engine is native Rust. Ordinary downloads never start another runtime.
- **Video sites:** video links go through bundled, unmodified copies of [yt-dlp](https://github.com/yt-dlp/yt-dlp) (which carries its own Python runtime), [Deno](https://deno.com) (a JavaScript runtime yt-dlp needs for YouTube) and an LGPL build of [ffmpeg](https://ffmpeg.org). They make the app much larger: about 250 MB per architecture instead of a few MB. Licences: [docs/THIRD_PARTY.md](docs/THIRD_PARTY.md).
- **Modern:** the interface is built with web technology inside a native window.
- **One codebase** for Windows and macOS, and the Chrome extension can share interface code with the app.

## Roadmap

- **Now:** desktop app for Windows and macOS
- **Later:** Android is not planned for now and may be revisited in the future

## Release

A release is built by GitHub Actions from a `vX.Y.Z` tag on `main`, as a draft that is published only after hand checks on its files. Steps for the maintainer: [docs/release.md](docs/release.md). Changes: [CHANGELOG.md](CHANGELOG.md).

## License

MIT. See [LICENSE](LICENSE).
