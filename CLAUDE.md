# ks-download-manager

Free download manager for images, audio and video, with up to 20× faster downloads. Part of KS-Suite; the suite-wide rules in the workspace `CLAUDE.md` apply.

## Stack
- Targets: Windows and macOS desktop. A Chrome extension is a possible add-on. No Android build for now (kept for the future).
- Rust for the download engine and all file and network work. Tauri for the desktop shell. TypeScript + React for the interface, with Tailwind CSS, shadcn/ui components and Lucide icons.
- Chrome extension, if built: TypeScript, Manifest V3, in `extension/`. It only hands URLs to the desktop app; downloading stays in Rust.
- The suite's Kotlin, Gradle and package-root rules do not apply here. Bundle identifier: `com.ks.downloadmanager`.
- Planned layout: `src-tauri/` (Rust), `src/` (interface), `extension/` (Chrome).
- Generated 2026-11-21 (Stage 1). Toolchain: Rust 1.100 (rustup, stable), Node 26 / npm 11. Tauri 2.12, React 19.3, TypeScript 6.0, Vite 8.3, Tailwind CSS 4.3, shadcn/ui (CLI 4.23, style radix-nova, Radix base), lucide-react 1.77. Full list with licences: `docs/THIRD_PARTY.md`.
- Toolchain is pinned: `rust-toolchain.toml` (Rust 1.100 + clippy) and `engines` in `package.json` (Node >= 26, npm >= 11).
- Theme: light/dark follows the system through a `prefers-color-scheme` media query in `src/index.css` (no JS). The main window starts hidden and Rust shows it after setting a matching background, so there is no white flash.
- TypeScript: `tsconfig.app.json` (src) and `tsconfig.node.json` (vite.config.ts), both checked by `tsc -b` in `npm run build`.
- CSP (`tauri.conf.json`): `connect-src ipc: http://ipc.localhost` for Tauri IPC. `dangerousDisableAssetCspModification: ["style-src"]` stops Tauri adding a style nonce, which would make browsers ignore `'unsafe-inline'` and block the `<style>` tag Radix dialogs inject. Scripts keep Tauri's hashes and nonces.
- Version placeholder: `src-tauri/Cargo.toml` has `version = "0.0.0"` because the Tauri CLI requires the field; `tauri.conf.json` and `package.json` have no version. Release builds pass the tag version (e.g. `tauri build --config '{"version":"1.2.3"}'`).
- Build stages: `docs/spec/stages.md`. One stage per request.

## Commands
```bash
npm install                      # first time
npm run tauri dev                # run the app with hot reload
npm run build                    # type-check (tsc -b, incl. vite.config.ts) + build the interface
cd src-tauri && cargo test       # Rust tests
cd src-tauri && cargo clippy --all-targets -- -D warnings
npm run tauri build              # release bundle (.app/.dmg on macOS)
```

## UI
- Follow `docs/spec/ui.md` (IDM-style single window: toolbar, sidebar, download list). Read it before any interface work.
- The design is our own: never copy icons, skins, screenshots or code from the reference apps.

## Clean room (non-negotiable)
- Follow `docs/spec/clean-room.md`. Never read, copy or adapt source code, binaries or assets of other download managers. Implement only from our own specs and public standards.
- Every dependency is listed with its licence in `docs/THIRD_PARTY.md`.

## Engine and speed
- Follow `docs/spec/engine.md`. The "up to 20×" claim is only made with measurements recorded in `docs/benchmarks.md`; never state a speed-up that was not measured.

## Video and playlists
- Follow `docs/spec/video-playlists.md`: site support through yt-dlp as an unmodified sidecar, plus ffmpeg. No DRM bypass.

## Constraints
- Free: no ads, no analytics, no crash-reporting SDKs.
- Network access is used only for the downloads the user starts.
- No links to other KS apps. If one is added, record the contract in the suite README (Cross-Repository Relationships).
