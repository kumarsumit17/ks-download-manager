# ks-download-manager

Free download manager for images, audio and video, with up to 20× faster downloads. Part of KS-Suite; the suite-wide rules in the workspace `CLAUDE.md` apply.

## Stack
- Targets: Windows and macOS desktop. A Chrome extension is a possible add-on. No Android build for now (kept for the future).
- Rust for the download engine and all file and network work. Tauri for the desktop shell. TypeScript + React for the interface.
- Chrome extension, if built: TypeScript, Manifest V3, in `extension/`. It only hands URLs to the desktop app; downloading stays in Rust.
- The suite's Kotlin, Gradle and package-root rules do not apply here. Bundle identifier: `com.ks.downloadmanager`.
- Planned layout: `src-tauri/` (Rust), `src/` (interface), `extension/` (Chrome).
- Exact versions and libraries are not pinned yet: record them here when the project is first generated.

## Constraints
- Free: no ads, no analytics, no crash-reporting SDKs.
- Network access is used only for the downloads the user starts.
- No links to other KS apps. If one is added, record the contract in the suite README (Cross-Repository Relationships).
