# 2026-11-21

* **Update**: Stage 4 review. [Download engine](engine.md) and [UI direction](ui.md): downloads are written to `<final name>.ksdownload` and renamed on success; leftover `.ksdownload` files without a matching map are deleted at start-up. [UI direction](ui.md): start-up restore off the critical path deferred to Stage 8. [Build stages](stages.md): marking downloaded files as coming from the internet (macOS quarantine, Windows Mark of the Web) in Stage 8 or 9.
* **Update**: [UI direction](ui.md) gained "Stage 4 behaviour" (where the download list is saved, restore after restart, downloads that cannot be resumed, never overwriting, paths kept in Rust, per-connection bars) and "Deferred from Stage 4": clipboard pre-fill → Stage 8, folder picker → Stage 5, Zustand → Stage 5, TanStack Table/Virtual → Stage 8.
* **Update**: [Download engine](engine.md): progress snapshots report each connection's current range; `ProbeInfo::resumable()` (Stage 4).
* **Update**: [Download engine](engine.md), Stage 3 review: a connection out of retries retires and hands its range to the others (only the last one fails the download); the range map is written only with a strong validator and removed before a fresh start; the partial file is kept on internal errors.
* **Update**: [Download engine](engine.md) gained "Splitting, retry and resume in detail" (Stage 3): victim choice, retry policy and `Retry-After`, the `.ksdm` range map, pause and resume rules.
* **Update**: Specs moved from `docs/spec/` to `specs/` in the repository root and converted to an Open Knowledge Format (OKF) v0.2 bundle: frontmatter on every file, plus this log and `index.md`.
* **Creation**: [Dependencies](dependencies.md) added: third-party dependencies are kept to the minimum.
* **Update**: [Video and playlist downloads](video-playlists.md) and [UI direction](ui.md) gained the "Always get the best available quality" option.
* **Update**: [Build stages](stages.md) and [UI direction](ui.md) gained the Clear all toolbar action (Stage 1).
* **Creation**: [Build stages](stages.md) added with Stage 1.
* **Creation**: [Download engine](engine.md), [UI direction](ui.md), [Video and playlist downloads](video-playlists.md) and [Clean-room rule](clean-room.md) written.
