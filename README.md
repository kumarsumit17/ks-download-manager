# KS Download Manager

> Part of the [KS-Suite](https://github.com/kumarsumit17/KS-Suite).

A free download manager for images, audio and video, built for speed: up to 20× faster downloads. No ads and no tracking.

| | |
|---|---|
| **Platform** | Windows · macOS (desktop) |
| **Tech stack** | Rust (download engine) · Tauri (desktop shell) · TypeScript + React with shadcn/ui and Lucide icons (interface) |
| **Chrome extension** | TypeScript, Manifest V3 (under consideration) |
| **Status** | Planned: this repository holds the project structure; app code is still to come |

## Planned features

- Download images, audio and video
- Download single videos and whole playlists from YouTube and other video sites ([spec](docs/spec/video-playlists.md))
- Up to 20× faster downloads than a single connection, by fetching each file over many connections at once ([how](docs/spec/engine.md))
- Free, with no ads, analytics or tracking

## Interface

A clean, classic single-window layout: toolbar on top, categories and queues on the left, the download list in the middle, in light and dark themes. See [docs/spec/ui.md](docs/spec/ui.md).

## Clean-room implementation

This is an independent implementation: no code or assets from other download managers. See [docs/spec/clean-room.md](docs/spec/clean-room.md).

You are responsible for having the right to download what you request. The app does not bypass DRM or other copy protection.

## Why this stack

- **Fast:** the download engine is native Rust, and the app starts quickly with low memory use because it bundles no language runtime.
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
