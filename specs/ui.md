---
type: Specification
title: UI direction
description: The main window, dialogs, look and feel, UI stack and quality bar for the desktop interface.
tags: [ui, design, accessibility]
---

# UI direction

Goal: a clean, classic download-manager window in the spirit of Internet Download Manager (one window, everything visible, nothing hidden behind menus), with a modern look and fast response.

## References
- [Internet Download Manager](https://www.internetdownloadmanager.com/): the layout to follow (toolbar, categories, download list) and the feature baseline (segmented downloads, resume, browser hand-off, batch, scheduler and queues, categories, light and dark themes).
- [AB Download Manager](https://github.com/amir1376/ab-download-manager): reference for a modern take on the same layout, with themes, queues and a browser extension that lives in its own repository. Apache-2.0, Kotlin; use it for ideas only, do not copy its code or assets.
- [Free Download Manager](https://www.freedownloadmanager.org/): **the** reference for feature scope and overall feel from Stage 5 on (segmented downloads, resume, browser integration, scheduler and queues, video, wide language support). Learned only by using it and reading its public pages.

The design must be our own: our own UI and icons, and no code, assets, screenshots or wording copied from any of these or any other download manager. Site rules in the Chrome extension (which element holds a video, how a site names its largest image) are written from observing the public pages themselves, never from another extension's code. The full rule is in [clean-room](clean-room.md).

## Main window

```
┌──────────────────────────────────────────────────────────────────────┐
│ ＋ Add URL  ▶ Resume  ⏸ Pause  ✕ Delete  ⌫ Clear all  ⏱ Queues  ⚙ Settings 🔍│  toolbar
├───────────────┬──────────────────────────────────────────────────────┤
│ All downloads │ Name            Size    Status    Speed   Time left  │
│  Unfinished   │ ──────────────────────────────────────────────────── │
│  Finished     │ holiday.mp4     1.2 GB  ███░ 64%  18 MB/s  0:42      │
│ Categories    │ album.zip       310 MB  Paused                       │
│  Video        │ cover.png       2.1 MB  Finished                     │
│  Audio        │                                                      │
│  Images       │                                                      │
│  Other        │                                                      │
│ Queues        │                                                      │
│  Main queue   │                                                      │
├───────────────┴──────────────────────────────────────────────────────┤
│ 2 active · ↓ 18 MB/s · 3 finished                                    │  status bar
└──────────────────────────────────────────────────────────────────────┘
```

- **Toolbar:** a single row of large, labelled actions. Actions that do not apply to the current selection are disabled, not hidden.
- **Clear all:** removes every download from the list after a confirmation dialog, stopping any still in progress. Files already saved on disk are kept. Disabled when the list is empty.
- **Sidebar:** All / Unfinished / Finished, then categories by file type (Video, Audio, Images, Other), then queues. Selecting an entry filters the list.
- **Download list:** a sortable table. Columns: Name, Size, Status (with an inline progress bar), Speed, Time left, Date added. Double-click opens a finished file; right-click gives Open, Open folder, Resume, Pause, Copy link, Delete.
- **Status bar:** active count, total speed, finished count.

## Dialogs
- **Add download:** paste a URL (pre-filled from the clipboard when it holds a link); shows file name, size, category, quality (for video, audio and multi-size images) and save folder before starting. Buttons: Download now, Download later.
- **Download progress:** per-download window or panel with speed, time left, and one bar per connection so the user can see the segments working.
- **Playlist picker:** shown when the pasted link is a playlist: entries with title and duration, a Select all box at the top (everything selected by default, with a selected count), one quality choice for all. See [video-playlists](video-playlists.md).
- **Settings:** General, Downloads (connections per file, speed limit, default folders per category, **Always get the best available quality**: on by default, see [video-playlists](video-playlists.md#best-available-quality)), Browser integration, Appearance (light, dark, follow system).

## Stage 4 behaviour
- **Download list on disk:** `downloads.json` in the app data folder (macOS `~/Library/Application Support/com.ks.downloadmanager/`, Windows `%APPDATA%\com.ks.downloadmanager\`), replaced atomically on every status change (temp file, sync, rename). A damaged file is set aside as `downloads.json.bad`.
- **In-progress file:** a download is written to `<final name>.ksdownload` (range map `<final name>.ksdownload.ksdm`) and renamed to the final name when it finishes. Pause, resume, delete and Clear all work on the `.ksdownload` file. An empty file at the final name holds the name meanwhile; on success it is replaced, but if it is no longer our empty file the download is saved as "name (n).ext" instead.
- **After a restart:** a download that was running or paused comes back **Paused** when the range map of its `.ksdownload` file matches (`engine::saved_state`), otherwise **Interrupted** (Resume starts it over). A leftover `.ksdownload` file of a download with no matching range map is deleted. Nothing starts by itself.
- **Downloads that cannot be resumed** (no range support, unknown size or no strong validator): the Add dialog says so before starting, the row shows "Can't pause", Pause is disabled with the reason, and Delete, Clear all and Quit say that what was downloaded is lost. Rust refuses to pause them.
- **Never overwrite:** if the destination exists and is not this download's own (its empty reservation, or its `.ksdownload` file with a matching `.ksdm`), the file is saved as "name (1).ext" and so on. A name whose `.ksdownload` file or range map exists counts as taken. Server-supplied names are made safe (no path separators, no "..", no reserved names) and the save folder must exist.
- **Paths stay in Rust:** open, open folder, pause, resume and delete name a download by ID; the path is looked up in Rust and passed to the system opener as an argument, never through a shell.
- **Per-connection view:** one bar per connection showing how far it is through its current range (the engine reports `start`, `position`, `end` for each connection), plus its speed.
- **Clear all:** finished files stay on disk; partial files of unfinished downloads are deleted with them.

## Deferred from Stage 4
- **Clipboard pre-fill in Add download** → Stage 5, Part 7 (Polish). Reading the clipboard from the web view shows a system paste prompt on macOS; doing it properly needs a clipboard plugin, to be justified then.
- **Folder picker for the save folder** → Stage 5, Part 3, with the per-category save folders and the settings screen. Until then the folder is a text field, defaulting to the system Downloads folder.
- **Zustand** → Stage 5, Part 3, when queues and settings add shared state. Stage 4 uses a small hand-written store (`src/features/downloads/store.ts`).
- **TanStack Table + TanStack Virtual** → Stage 5, Part 7, with the 10,000-row scroll test. Stage 4 keeps the plain sortable table.
- **Start-up restore off the critical path** → Stage 5, Part 7. Stage 4 reads the range map of every unfinished download before the window shows; fast for a normal list, but it adds to start-up time with many unfinished downloads (quality bar: usable window in under 1 second).

## Look and feel
- Flat and calm: one accent colour, generous spacing, no gradients or skeuomorphic buttons.
- Light and dark themes from day one, following the system setting by default.
- Native window chrome and menus on each platform (Windows title bar, macOS traffic lights and menu bar).
- Fully usable from the keyboard: Ctrl/Cmd+N add, Space pause/resume, Delete remove, Ctrl/Cmd+F search.

## UI stack
- **React + TypeScript + Vite** for the interface.
- **Tailwind CSS** with design tokens (colour, spacing, radius, type scale) defined once, so light and dark themes and the accent colour come from one place.
- **[shadcn/ui](https://ui.shadcn.com/)** for components (buttons, dialogs, menus, tick boxes, tooltips, table, progress, tabs). Its components are added to our repo as source files, so we own and restyle them; they bring correct keyboard and screen-reader behaviour. Add only the components actually used.
- **Our own theme on top:** set the accent colour, radius and spacing tokens ourselves so the app does not look like an unmodified shadcn template.
- **Virtualised table** (TanStack Table + TanStack Virtual) for the download list.
- **Small state store** (Zustand); download state lives in Rust and is streamed to the UI.
- **Motion kept short:** 120-200 ms transitions on progress, row insert and dialogs; honour the system "reduce motion" setting.
- **Native feel:** system font on each platform, native window controls, and translucent window material where the system offers it (Mica on Windows 11, vibrancy on macOS), with a solid fallback.
- **Icons: [Lucide](https://lucide.dev/)** (the set shadcn/ui uses by default), one consistent stroke weight and size throughout. No icons from other download managers.
- shadcn/ui and Lucide are open-source dependencies: record both with their licences in `docs/THIRD_PARTY.md`.
- Exact libraries and versions are confirmed when the project is generated and recorded in `docs/THIRD_PARTY.md`.

## Quality bar
- Starts to a usable window in under 1 second on a mid-range laptop.
- 60 fps scrolling with 10,000 rows in the list.
- Contrast meets WCAG AA in both themes; every action reachable by keyboard.
- Crisp at 100 %, 150 % and 200 % display scaling.

## Performance rules
- The window is interactive before the download list finishes loading.
- The list is virtualised, so thousands of rows scroll smoothly.
- Progress updates are batched (a few times a second), never once per network chunk.

## Out of scope for the first version
Torrents, capturing video from inside the browser's player, scheduler-driven shutdown, custom skins.

Video and playlist downloads by link are in scope: see [video-playlists](video-playlists.md).
