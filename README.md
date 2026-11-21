# KS Download Manager

> Part of the [KS-Suite](https://github.com/kumarsumit17/KS-Suite).

A free download manager for images, audio and video, built for speed: up to 20× faster downloads. No ads and no tracking.

| | |
|---|---|
| **Platform** | Windows · macOS (desktop) |
| **Tech stack** | Rust (download engine) · Tauri (desktop shell) · TypeScript + React (interface) |
| **Chrome extension** | TypeScript, Manifest V3 (under consideration) |
| **Status** | Planned: this repository holds the project structure; app code is still to come |

## Planned features

- Download images, audio and video
- Up to 20× faster downloads
- Free, with no ads, analytics or tracking

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
