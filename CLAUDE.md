# ks-download-manager

Free download manager for images, audio and video, with up to 20× faster downloads. Part of KS-Suite; the suite-wide rules in the workspace `CLAUDE.md` apply.

## Stack
- Targets: Windows and macOS desktop. A Chrome extension is a possible add-on. No Android build for now (kept for the future).
- Kotlin. Desktop UI toolkit and download libraries are not decided yet: record them here before writing app code.
- Package: `com.ks.downloadmanager`

## Constraints
- Free: no ads, no analytics, no crash-reporting SDKs.
- Network access is used only for the downloads the user starts.
- No links to other KS apps. If one is added, record the contract in the suite README (Cross-Repository Relationships).
