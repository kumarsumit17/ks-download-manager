# ks-download-manager

Free download manager for images, audio and video, with up to 20× faster downloads. Part of KS-Suite; the suite-wide rules in the workspace `CLAUDE.md` apply.

## Stack
- Targets: Windows and macOS desktop. A Chrome extension is a possible add-on. No Android build for now (kept for the future).
- Rust for the download engine and all file and network work. Tauri for the desktop shell. TypeScript + React for the interface, with Tailwind CSS, shadcn/ui components and Lucide icons.
- Chrome extension, if built: TypeScript, Manifest V3, in `extension/`. It only hands URLs to the desktop app; downloading stays in Rust.
- The suite's Kotlin, Gradle and package-root rules do not apply here. Bundle identifier: `com.ks.downloadmanager`.
- Planned layout: `src-tauri/` (Rust), `src/` (interface), `extension/` (Chrome).
- Exact versions and libraries are not pinned yet: record them here when the project is first generated.

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
