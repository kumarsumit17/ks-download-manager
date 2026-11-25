---
okf_version: "0.2"
---

# Plan

* [Build stages](stages.md) - The nine build stages, what each delivers, and the checks that must pass before a stage is committed.
* [Per-platform optimization plan](plan-per-platform.md) - Approved plan for making the macOS and Windows builds each the best on its OS, released together under one version.

# Product specifications

* [UI direction](ui.md) - The main window, dialogs, look and feel, UI stack and quality bar for the desktop interface.
* [Download engine](engine.md) - How the Rust engine downloads a file over many connections, resumes safely, and how the speed claim is proven.
* [Video and playlist downloads](video-playlists.md) - Single-video and playlist downloads through yt-dlp and ffmpeg, the best-available-quality option, and their limits.
* [Chrome extension and hand-off](extension.md) - What the Chrome extension does, its permissions, and how it hands URLs to the desktop app over native messaging restricted to our extension ID.

# Policies

* [Dependencies](dependencies.md) - Third-party dependencies are kept to the minimum; the order of preference and what must be reported for each new one.
* [Clean-room rule](clean-room.md) - What may and may not be used from other download managers, and the working practice that keeps the implementation independent.
