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
- **Download list:** a sortable table. Columns: Name, Size, Status (with an inline progress bar), Speed, Time left, Date added. Double-click opens a finished file; right-click gives Open, Open folder, Resume, Pause, Move to queue, Copy link, Delete (and Resume playlist, Pause playlist on a playlist's entry). A playlist's entries are grouped under a heading row (see Playlists below).
- **Status bar:** active count, total speed, finished count.

## Dialogs
- **Add download:** paste a URL (pre-filled from the clipboard when it holds a link); shows file name, size, category, quality (for video, audio and multi-size images) and save folder before starting. Buttons: Download now, Download later.
- **Download progress:** per-download window or panel with speed, time left, and one bar per connection so the user can see the segments working.
- **Playlist picker:** shown in Add download when the pasted link is a playlist: entries with thumbnail, position, title and duration, a Select all box at the top (everything available selected by default, with "n of m selected"), shift-click for ranges, unavailable entries marked and not selectable. Best available quality for every entry, with no quality choice (Part 5 decision). See [video-playlists](video-playlists.md#how-a-playlist-runs-stage-5-part-5).
- **Settings:** General, Downloads (connections per file, speed limit, default folders per category, **Always get the best available quality**: on by default, see [video-playlists](video-playlists.md#best-available-quality)), Browser integration, Appearance (light, dark, follow system).

## Stage 5 behaviour (Part 3: scheduler, queues, categories, settings)
- **Waiting:** Resume (or Download now) means "wants to run". A download starts when fewer than the **Downloads at once** limit run; otherwise it shows **Waiting** and starts in turn: the main queue first, then the other queues in their order, and within a queue by date added. Videos count toward the limit. Pause on a waiting download ends its wait (it becomes Paused if it has bytes, else Queued). After a restart a waiting download comes back Paused, Queued or Interrupted like any other; nothing starts by itself.
- **Queues:** the **Queues** toolbar button opens a dialog to create, rename and delete queues (names 1 to 60 characters, unique), and to start or stop each queue, the main queue included. Deleting a queue moves its downloads to the main queue and stops nothing. Start makes every unfinished download of the queue wait its turn; Stop pauses its running downloads and ends its waits; downloads that cannot be paused keep running, and the app says how many. Right-clicking a queue in the sidebar gives Start and Stop; a download's menu has **Move to queue**. Queues are saved with the list in `downloads.json`.
- **Categories and save folders:** each category (Video, Audio, Images, Other) can have its own save folder in Settings; without one it is the system Downloads folder. The Add dialog switches to the category's folder once the link is looked up, unless the user typed or chose a folder. Downloads handed over by the browser extension go to their category's folder.
- **Folder picker:** **Choose…** next to a folder opens the system picker. macOS: `osascript` runs a fixed script, and the prompt and start folder are passed as arguments, never inserted into the script. Windows: PowerShell's folder dialog, with the values in environment variables (written, untested).
- **Settings** (`settings.json` in the app data folder, replaced atomically; a damaged file is set aside as `settings.json.bad`):
  - Limits: downloads at once (1 to 20, default 3), connections in total (1 to 256, default 64), connections per download (1 to 32, default 16; a download starts with a few and adds more while the speed rises, see [engine](engine.md#adaptive-connections)), speed limit in KB/s or none. Beside the speed limit, Settings says that each video download paces itself, so several videos downloading together can go over it ([video-playlists](video-playlists.md)). Lower limits apply as running connections and downloads end.
  - Save folders per category; a folder must exist.
  - Video: **Prefer MP4**, and **Update video component** (also offered on a failed video's details).
  - **Leftover download files** (below).
- **Leftover download files:** unfinished files of downloads no longer in the list (for example removed while the app was not running, or after a crash), and the empty reservation files that held their final names.
  - Ownership is recorded when they are made: every video temp folder holds `ks-owner.json`; every `.ksdownload` file gets `<file>.ksowner` beside it (app identifier, download ID, URL, creation time), removed when the download finishes or its partial file is deleted. The empty reservation file that holds a download's final name gets the same marker, `<final name>.ksowner`, when the name is reserved; it is removed when the finished file takes the name or the download leaves the list. A name with a `.ksowner` file beside it is never reserved again. A valid `.ksdm` range map also counts. An empty file without our marker is never listed.
  - A few seconds after start-up the app looks **only** in the folders it knows: the system Downloads folder, the folders of downloads in the list, and the category folders, one level deep. Entries that are links are skipped, so nothing outside those folders is reached.
  - Settings shows the number found, their total size and the list. Nothing is listed unless it carries our marker or a valid map, and nothing that belongs to a download in the list. Nothing is removed automatically.
  - Each item has a tick box, all ticked when the list appears. **Remove** removes only the ticked items, after a confirmation that names their number and size. Each item is checked again at that moment (same kind of entry, same marker or a valid map, a reservation still empty, still not in the list); anything that changed is kept. A temp folder is removed with what is inside it; links inside are removed as links, never followed.

## Languages (Stage 5, Part 4)
- **English and Hindi**, through ks-utility ([languages](https://github.com/kumarsumit17/ks-utility/blob/main/specs/languages.md), [format](https://github.com/kumarsumit17/ks-utility/blob/main/specs/translation-format.md)). Every interface string is a key: app text in `shared/src/locales/en.json` and `hi.json`, common words (`common.*`) from ks-utility's shared catalogue, looked up by `t()` in `shared/src/lib/i18n.ts`. No text is written into components.
- **Language rule:** Settings → Language offers "Same as the system" (the default), English and Hindi, saved in `settings.json` (`language`). The saved language is applied before the window is drawn; changing it redraws the window.
- **Rust sends message codes, never sentences:** errors and a download's problem are `{ code, params }` (`shared/src-tauri/src/msg.rs`), shown with `t(code, params)`. Engine errors map to `engine.*` codes; the engine's own English text is only for logs. A problem saved by an older version as an English sentence is shown as it was (`error.savedText`). Parameters such as `{detail}` carry the system's own error text (for example from the file system), which stays in the system's language.
- **Formatting:** sizes, speeds, times left and dates go through ks-utility's formatters and `Intl` for the language, always with Latin digits; unit symbols stay (`KB`, `MB/s`).
- **Checks:** the app runs ks-utility's language-file check on `shared/src/locales` and the extension's `_locales` (`npm run test:lang`), and a Rust test fails when a code Rust sends has no text, or its text lacks a parameter Rust fills in. `docs/hindi-review.md` lists every string for the owner's review (`npm run hindi-review`).

## Playlists (Stage 5, Part 5)
- **Picker:** in Add download, a playlist link shows the picker instead of the file-name field: the playlist's title, **Select all** with "n of m selected", then one line per entry (tick box, thumbnail, position, title, duration, or "Unavailable"). Every available entry is ticked when it opens; a click toggles one, a shift-click sets the whole range from the last clicked box to the same state. **Start each file name with its number in the playlist** (on by default) and the save folder sit below it; the note says the files go into a folder named after the playlist inside that folder. One button: **Download n videos**.
- **Thumbnails** are fetched by Rust (`playlist_thumbnail`), only for a playlist looked up in this session, only over `https`, never through a redirect, within 15 s, at most 4 at once over one shared client, at most 1 MB, and only JPEG, PNG, WebP or GIF; the page turns the bytes into a `data:` URL, so the content policy stays `img-src 'self' data:`. The picker is windowed like the list (only the entries in view, plus a few, are drawn); a thumbnail is asked for when its entry is drawn, and the last 300 are kept so scrolling back does not fetch them again.
- **List:** each entry is its own row, indented under a heading row with the playlist's title, a fold button, the entry count, "finished of total" as a bar and a number (red when an entry failed), the combined speed, and **Resume playlist** (resumes paused and queued entries and retries failed ones) and **Pause playlist**. Within the group, rows are in the playlist's order; the group sits where its first row falls in the current sort. A folded group's rows are skipped by the arrow keys. The heading is not a download and is never selected; the group goes away with its last entry.

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
- **Clipboard pre-fill in Add download** → done in Stage 5, Part 7 (below): read by Rust with the system `pbpaste`, not by the web view, so no clipboard plugin was needed.
- **Folder picker for the save folder** → done in Stage 5, Part 3 (above).
- **Zustand** → not added in Stage 5, Part 3: queues are one more list mirrored from Rust events in the same hand-written store (`shared/src/features/downloads/store.ts`), and settings are read and saved by the Settings dialog alone.
- **TanStack Table + TanStack Virtual** → not added in Stage 5, Part 7: rows have one fixed height, so our own windowing (`shared/src/features/downloads/windowing.ts`) draws only the rows on screen (below).
- **Start-up restore off the critical path** → done in Stage 5, Part 7 (below).

## Polish (Stage 5, Part 7)
- **First-run notice:** on the first start, a dialog says that the user is responsible for what they download, that the app does not remove or get around copy protection (DRM) and does not support paid streaming or sign-in videos, and that it is free and uses the network only for the downloads the user starts, plus a daily check for a new version of the app that can be turned off in Settings. Only **I understand** closes it (not Escape); the choice is saved in `settings.json` (`noticeSeen`) by its own command, so a missing category folder cannot bring it back.
- **Clipboard pre-fill** (Settings → Add download → **Fill in a link from the clipboard**, on by default; off means the clipboard is never read): when Add download opens and the link field is empty, Rust reads the clipboard (`pbpaste` on macOS; PowerShell `Get-Clipboard` on Windows, untested) and passes it to the page only when it is exactly one `http(s)` link of at most 2048 characters, with no spaces and no user name or password. The text is selected, so typing replaces it. Nothing else on the clipboard reaches the page, and nothing is kept. The clipboard is read only when the dialog opens.
- **10,000 rows:** the list is windowed. Every row (download or playlist heading) has one height, measured from a drawn row; only the rows on screen plus 8 above and below are drawn, and two spacer rows keep the full scroll height. The arrow keys move through every row; the selection is scrolled into view by its position, only when the selection changes (progress updates never pull the list back to it). The list is re-sorted and re-grouped only when its order can change (a row's id, status, playlist or sort-column value), not on every progress tick. `aria-activedescendant` is set only while the selected row is drawn. The table carries `aria-rowcount` and each row `aria-rowindex`, so screen readers know the real size.
- **Start-up:** the window opens before the saved list is checked: the range maps are read in the background, the list is sent when that is done, and nothing starts before it. If that check fails (a panic), the list is shown as it was loaded, so the window never stays empty. A download handed over by the extension meanwhile is left as it is.
- **Keyboard:** besides Ctrl/Cmd+N, Ctrl/Cmd+F, Space, Delete and the arrow keys, Shift+F10 (or the Menu key) opens the row menu at the selected row. Every toolbar, sidebar, dialog and playlist-heading control is a real button or field, so Tab reaches it.
- **Contrast** (computed from the theme's OKLCH colours, alpha blended in sRGB as browsers do): every text and background pair meets WCAG AA (at least 4.5:1) in both themes. The lowest are light error text on a selected row (4.94:1) and light button text on the accent colour (5.04:1).
- **Reduced motion:** the spinner and the indeterminate progress bars stop when the system asks for reduced motion (CSS `prefers-reduced-motion`).
- **Window material:** not done. Tauri's window effects need a transparent window, which on macOS needs Tauri's private-API flag (`macos-private-api`); the window keeps its solid background.

## The system's own touches (per-platform plan, step 2)
- **Keep awake** (Settings → This computer → **Keep the computer awake while downloading**, `keepAwake`, on by default): while at least one download runs, the computer does not go to idle sleep; the display still may. macOS: a `caffeinate -i -w <app pid>` process holds the power assertion (it ends with the app, even after a crash); Windows: one thread holds `SetThreadExecutionState(ES_SYSTEM_REQUIRED)`. Released when the last download ends.
- **Dock and taskbar**: the progress of the running downloads (bytes so far out of the total of those whose size is known; indeterminate when none knows its size) on the Dock icon or the taskbar button, and on macOS the number running as the Dock badge; both cleared when none run, updated only when they change.
- **Where from** (macOS): a finished file records the link it came from (`com.apple.metadata:kMDItemWhereFroms`, a binary property list made by `plutil`), shown by Finder's Get Info and Spotlight, as for a Safari download.
- **Downloads stack** (macOS): a file saved in `~/Downloads` makes the Dock's Downloads stack bounce (`com.apple.DownloadFileFinished`).
- **Menu bar** (macOS; built in `desktop/mac`, labels from the language files and rebuilt when the language changes): the app menu (About, Settings… ⌘,, Check for Updates…, Services, Hide, Hide Others, Show All, Quit), **File** (Add Download… ⌘N, Open ⌘O, Show in Finder ⌘R, Delete… ⌘⌫, Close Window ⌘W), **Edit** (standard), **Downloads** (Resume, Pause, Queues…, Clear All…), **View** (All, Unfinished, Finished ⌘1–⌘3, Video, Audio, Images, Other ⌘4–⌘7, Search ⌘F), **Window** (Minimize, Zoom, Full Screen) and **Help** (the README on GitHub). An item does what its toolbar button or shortcut does, on the selected download where it needs one. Windows has no menu bar (the toolbar is the menu).

## App updates
- **Where:** Settings → **App updates** shows the running version ("built from source" for `0.0.0`), the switch **Check for updates automatically** (on by default, `updateCheck` in `settings.json`) and a **Check for updates** button. When a newer version is found, a bar above the download list says "Version X is available." with **Download and install (size)** and **Later**; Settings shows the same install button.
- **Shared rules:** the suite's [app-update behaviour](https://github.com/kumarsumit17/ks-utility/blob/main/specs/app-update.md): versions, the daily schedule, reading the release, which file and the checksum rule come from `ks_utility::app_update`, the interface state from `createUpdateStore`, and the texts from the shared `common.appUpdate.*` catalogue (ks-utility `v0.3.0`).
- **Check:** `GET https://api.github.com/repos/kumarsumit17/ks-download-manager/releases/latest`, unauthenticated, nothing else sent. It runs when the button is clicked, and 10 s after start-up and then every hour while the app runs, each time only when the switch is on, no check ran in the last 24 hours (`app-update.json` in the app data folder) and the build has a version (never for `0.0.0`). This daily request is the one exception to "network only for what the user starts", named in the first-run notice. A failed start-up check is only logged.
- **Which file:** by name from the release: `-macos-arm64.dmg` on Apple silicon, `-macos-x64.dmg` on Intel, `-windows-x64-setup*.exe` on Windows. Only `https://github.com/kumarsumit17/ks-download-manager/releases/download/` URLs, and only an asset with a `sha256:` digest from GitHub.
- **Install:** the installer is downloaded by our engine to `<app data>/app-update/`, its SHA-256 checked against GitHub's digest (a mismatch deletes it), marked as downloaded from the internet, and opened. macOS: the disk image opens and the user drags the app into Applications, then reopens it. Windows: the installer starts and the app quits (as with Quit, resumable downloads keep their place). No updater framework and no new dependency.
- **Chrome extension:** it never checks the network. The app's ping reply carries `update` (the newer version the app's last check found); the popup then says "Version X is available" with **Get the new version**, which opens the latest release page. The extension is loaded unpacked, so it is updated by loading the new zip.

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
