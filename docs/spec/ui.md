# UI direction

Goal: a clean, classic download-manager window in the spirit of Internet Download Manager (one window, everything visible, nothing hidden behind menus), with a modern look and fast response.

## References
- [Internet Download Manager](https://www.internetdownloadmanager.com/): the layout to follow (toolbar, categories, download list) and the feature baseline (segmented downloads, resume, browser hand-off, batch, scheduler and queues, categories, light and dark themes).
- [AB Download Manager](https://github.com/amir1376/ab-download-manager): reference for a modern take on the same layout, with themes, queues and a browser extension that lives in its own repository. Apache-2.0, Kotlin; use it for ideas only, do not copy its code or assets.
- [Free Download Manager](https://www.freedownloadmanager.org/): reference for feature scope (segmented downloads, resume, browser integration, wide language support).

The design must be our own: no copied icons, skins, screenshots or wording from any of these.

## Main window

```
┌──────────────────────────────────────────────────────────────────────┐
│  ＋ Add URL   ▶ Resume   ⏸ Pause   ✕ Delete   ⏱ Queues   ⚙ Settings   🔍 │  toolbar
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
- **Sidebar:** All / Unfinished / Finished, then categories by file type (Video, Audio, Images, Other), then queues. Selecting an entry filters the list.
- **Download list:** a sortable table. Columns: Name, Size, Status (with an inline progress bar), Speed, Time left, Date added. Double-click opens a finished file; right-click gives Open, Open folder, Resume, Pause, Copy link, Delete.
- **Status bar:** active count, total speed, finished count.

## Dialogs
- **Add download:** paste a URL (pre-filled from the clipboard when it holds a link); shows file name, size, category and save folder before starting. Buttons: Download now, Download later.
- **Download progress:** per-download window or panel with speed, time left, and one bar per connection so the user can see the segments working.
- **Settings:** General, Downloads (connections per file, speed limit, default folders per category), Browser integration, Appearance (light, dark, follow system).

## Look and feel
- Flat and calm: one accent colour, generous spacing, no gradients or skeuomorphic buttons.
- Light and dark themes from day one, following the system setting by default.
- Native window chrome and menus on each platform (Windows title bar, macOS traffic lights and menu bar).
- Fully usable from the keyboard: Ctrl/Cmd+N add, Space pause/resume, Delete remove, Ctrl/Cmd+F search.

## Performance rules
- The window is interactive before the download list finishes loading.
- The list is virtualised, so thousands of rows scroll smoothly.
- Progress updates are batched (a few times a second), never once per network chunk.

## Out of scope for the first version
Torrents, video grabbing from web players, scheduler-driven shutdown, custom skins.
