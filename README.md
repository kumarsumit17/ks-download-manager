# KS Download Manager

> Part of the [KS-Suite](https://github.com/kumarsumit17/KS-Suite).

A free download manager for images, audio and video, built for speed. The goal is downloads up to 20× faster than a single connection, to be confirmed by measurements before it is claimed. No ads and no tracking.

| | |
|---|---|
| **Platform** | Windows · macOS (desktop) |
| **Tech stack** | Rust (download engine) · Tauri (desktop shell) · TypeScript + React with shadcn/ui and Lucide icons (interface) |
| **Chrome extension** | TypeScript, Manifest V3 (under consideration) |
| **Status** | In development: Stages 1–4 are done; Stage 5 (Final) is being built part by part. See [stages](specs/stages.md) |

## Planned features

- Download images, audio and video
- Download single videos and whole playlists from YouTube and other video sites ([spec](specs/video-playlists.md))
- Faster downloads by fetching each file over many connections at once, aiming for up to 20× a single connection where the server allows it; numbers will be published in `docs/benchmarks.md` once measured ([how](specs/engine.md))
- Free, with no ads, analytics or tracking

## Interface

A clean, classic single-window layout: toolbar on top, categories and queues on the left, the download list in the middle, in light and dark themes. See [specs/ui.md](specs/ui.md).

## Building

```bash
npm install
npm run sidecars        # fetch yt-dlp and Deno, build ffmpeg (checked against pinned SHA-256)
npm run tauri dev       # run with hot reload
npm run bundle:mac      # release .app, then sign the bundled programs (ad-hoc unless an identity is given)
```

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
- **Maybe:** a Chrome extension that hands downloads to the desktop app
- **Later:** Android is not planned for now and may be revisited in the future

## Release

Releases will be cut by pushing a semantic version tag (`git tag v1.2.3 && git push --tags`).
See the suite-level [CI/CD guide](https://github.com/kumarsumit17/KS-Suite#-cicd-semantic-versioning--automated-signing).

## License

MIT. See [LICENSE](LICENSE).
