---
type: Plan
title: Per-platform optimization plan
description: Approved plan for making the macOS and Windows builds of KS Download Manager each the best on its OS (look, input, startup, packaging and size, OS integrations, accessibility), released together under one version.
tags: [plan, platforms, macos, windows, versioning, accessibility, performance]
---

# Per-platform optimization plan

**Status: approved by the owner on 2026-11-25, with the recommended answers in section 3:** paid signing left to the owner (both apps keep working and warn on first open until then), notifications through `tauri-plugin-notification`, two macOS disk images (Windows ARM64 later), Deno bundled, the WebView2 download bootstrapper kept, macOS first. Built steps are marked in section 4.

Goal: the macOS build is the best download manager on a Mac and the Windows build the best on Windows, instead of one build that treats both the same (suite rule, [platform folders](https://github.com/kumarsumit17/KS-Suite/blob/main/specs/platform-folders.md)). **Versions stay as they are:** one plain `vX.Y.Z` tag releases macOS, Windows and the Chrome extension together with one version; platform tags and bridge releases are not used for now (suite rule, KS-Suite `dev` 25e10b6), and ks-utility's platform-tag helpers stay unused. The download engine, the video support and the interface stay written once; what differs per OS moves to that platform's folder. The Chrome extension stays one product for both. Modelled on [ks-jaiplayer's plan](https://github.com/kumarsumit17/ks-jaiplayer/blob/dev/specs/plan-per-platform.md).

## 1. Audit: what each build does today (v0.1.5, 2026-11-25)

One Tauri codebase in the repository root (`src/`, `src-tauri/`, `extension/`) builds both apps. `desktop/mac/` and `desktop/windows/` hold only a `.gitkeep`, and there is no `shared/` yet ([CLAUDE.md](../CLAUDE.md), "Platform folders"). OS differences are `cfg` branches in 11 Rust files (`handoff.rs` 44, `video.rs` 12, `component.rs`, `manager.rs`, `picker.rs` 9 each, `mark.rs`, `open.rs` 8, …).

| | macOS | Windows |
|---|---|---|
| Builds | `.app` + dmg, arm64 and x64 (two dmgs), from `macos-15` CI | NSIS installer, x64 only, from `windows-2025` CI; **beta, never run on a real PC by hand** |
| Signing | Ad-hoc (`signingIdentity: "-"`); the workflow signs and notarises with a Developer ID only if the Apple secrets exist; they do not, so Gatekeeper blocks a browser-downloaded copy | **Unsigned**: SmartScreen warns on first run |
| Runtime the user needs | None (WKWebView is part of macOS 11+) | WebView2: `downloadBootstrapper`, so an install on a PC without WebView2 **needs the internet**; Windows 11 and current Windows 10 have it already |
| Look | Web interface in a standard window; light/dark follows the system; no window material | Same; title bar follows the system theme; no Mica |
| Menu bar | **Tauri's default app menu only**: no File / Edit / View / Window / Help with the app's own commands | No menu (none expected); the toolbar is the menu |
| Keyboard | App shortcuts in the web layer (Ctrl/⌘, Delete, Shift+F10 for the row menu) | Same |
| Dock / taskbar | Nothing: **no progress, no badge** with the active count | Nothing: **no taskbar progress**, no overlay icon, no jump list |
| Notifications | **None** when a download finishes or fails | **None** |
| Sleep | **Not prevented** while downloads run (a long download stops when the Mac sleeps) | **Not prevented** |
| Downloaded files | Quarantine attribute (`mark.rs`), so Gatekeeper checks them; **no "Where from"** (`kMDItemWhereFroms`); the Downloads stack does not bounce | Mark of the Web (`Zone.Identifier`), **untested** |
| Browser | Chrome native host manifest written on every start; a Unix socket in app data | Chrome host registered in HKCU by the installer; a named pipe. **Edge is not registered** (Edge loads the same unpacked extension) |
| Single instance | Socket is the lock; a second copy shows the first window | Pipe is the lock; same |
| File types and links | None (no `.ksdm` files, no URL scheme); drag and drop of a link onto the window is not supported | None; same |
| Startup | Window starts hidden, shown after the theme colour is set; no measurement recorded | Same; no measurement |
| Size | App about 260 MB (local build): yt-dlp 124 MB, Deno 78 MB (arm64) / 94 MB (x64), ffmpeg + ffprobe about 45 MB | Installer 74 MB (compressed) |
| Version and updates | One tag `vX.Y.Z` builds and releases both and the extension; the updater (ks-utility v0.9.0 helpers) reads `releases/latest`; this stays | Same |
| Accessibility | ARIA on the windowed list (row count, active descendant); keyboard reaches everything; contrast AA in both themes; Reduce Motion honoured. **VoiceOver not tested** | Same; **Narrator not tested**; Windows High Contrast (forced colours) not handled |

## 2. The plan, per platform

New dependencies are reported as [Dependencies](dependencies.md) requires. Everything listed without one uses what is already in `Cargo.lock` (Tauri 2.12 has `set_progress_bar` and `set_badge_count`; `windows-sys` is already a Windows dependency) or the system's own tools.

### 2.0 Layout first (`shared/`, `desktop/mac/`, `desktop/windows/`)

- Move the Tauri codebase into `shared/` (`shared/src/`, `shared/src-tauri/`, `shared/extension/`), as [CLAUDE.md](../CLAUDE.md) already plans, with the paths in the workflows, scripts, graphify and `package.json` updated in the same change.
- Per-platform Rust code moves out of `cfg` branches into `desktop/mac/` and `desktop/windows/` as small crates (or modules included by path) that implement one `Platform` trait in `shared/`: open/reveal, folder picker, mark downloaded file, keep awake, Dock/taskbar progress, notifications, browser host registration. The engine and manager call only the trait.
- Per-platform Tauri config: `tauri.macos.conf.json` and `tauri.windows.conf.json` (Tauri merges them), so each OS has its own bundle settings, window options and identifiers in its own folder.

### 2.1 macOS (`desktop/mac/`)

- **Look and input**
  - A full **menu bar** to macOS conventions: KS Download Manager (About, Settings ⌘,, Check for Updates, Hide, Quit), **File** (Add Download ⌘N, Add from Clipboard ⇧⌘V, Open ⌘O, Show in Finder ⌘R, Delete ⌫, Close Window ⌘W), **Edit** (standard, so ⌘C/⌘V work in fields), **Downloads** (Resume, Pause, Start Queue, Stop Queue), **View** (categories ⌘1–⌘5, Show Details), **Window** (Minimize ⌘M, Zoom), **Help** (opens the README). Built with Tauri's menu API, text from the language files.
  - Window material: vibrancy (`NSVisualEffectView` sidebar material) behind the sidebar where the system allows it, and a unified title bar with the toolbar (`titleBarStyle: Overlay`), checked for contrast in both themes.
  - **Drag and drop**: a link or a `.webloc` dropped on the window or the Dock icon opens Add download.
- **Performance and startup**: measure cold start to the first painted list on Apple silicon (target under 1 s) and record it in `docs/benchmarks.md`; keep the list restore after the window shows (already).
- **Packaging and size**: one universal dmg or keep two (owner, 3.3); Deno only where it is needed (owner, 3.4); report the app size per release.
- **OS integrations**
  - **Dock progress and badge**: the total progress of running downloads on the Dock icon (`set_progress_bar`) and the number running as the badge (`set_badge_count`), cleared when none run.
  - **Notifications** when a download finishes or fails, clicking one shows it in the list (owner, 3.2: new dependency or the system's own API).
  - **Keep awake** while downloads run: an IOKit power assertion (`PreventUserIdleSystemSleep`, through `caffeinate -i -w <pid>` or the IOKit call), released when the last one ends. A Settings switch, on by default.
  - **"Where from"**: set `kMDItemWhereFroms` (the link and its page) on finished files, as Safari does, so Finder's Get Info and Spotlight show the source.
  - **Downloads stack bounce**: post `com.apple.DownloadFileFinished` for files saved in `~/Downloads`, so the Dock's Downloads stack bounces.
  - Services menu: "Download with KS Download Manager" for a selected link in any app.
- **Signing**: Developer ID signing and notarisation (owner, 3.1), so a browser-downloaded copy opens without the Gatekeeper block and the in-app updater's dmg opens cleanly.
- **Accessibility**: a VoiceOver pass over the toolbar, sidebar, list, details and every dialog (labels, the list's row announcements, progress read as a percentage), recorded in `docs/`; the menu bar makes every command reachable by keyboard and VoiceOver.

### 2.2 Windows (`desktop/windows/`)

- **Look and input**
  - **Mica** backdrop on Windows 11 (`windowEffects: mica`), falling back to the plain theme colour on Windows 10; the title bar follows the app's light/dark theme.
  - Windows conventions: Ctrl+N add, Ctrl+V paste a link into a new download, Delete, Enter opens, F2 renames where it applies, Alt reaches the toolbar's access keys, Shift+F10 and the Menu key open the row menu (already).
  - Drag and drop of a link or an `.url` file onto the window.
- **Performance and startup**: the same measurement as macOS on CI and on the owner's PC; WebView2 warm-up kept off the critical path.
- **Packaging and size**
  - **WebView2 without the internet** (owner, 3.5): embed the offline bootstrapper/installer, or keep the download bootstrapper (Windows 11 always has WebView2).
  - Per-user install kept (no admin prompt); Start menu and desktop shortcut choices in the installer; an **ARM64** build (owner, 3.3).
- **OS integrations**
  - **Taskbar progress** (`set_progress_bar`: normal, paused, error states) and an **overlay icon** with the number running.
  - **Toast notifications** on finish and failure (owner, 3.2), with "Open" and "Show in folder".
  - **Keep awake** while downloads run: `SetThreadExecutionState(ES_SYSTEM_REQUIRED | ES_CONTINUOUS)` (`windows-sys`, already a dependency), released when the last one ends; the same Settings switch.
  - **Jump list**: Add download, Open Downloads folder, the last five finished files.
  - **Mark of the Web** checked on a real PC (SmartScreen and Office honour it), with the source link in `Zone.Identifier` (`ReferrerUrl`, `HostUrl`).
  - **Edge** registered as a host too (`HKCU\Software\Microsoft\Edge\NativeMessagingHosts\…`), so the same extension works in Edge.
- **Signing**: an Authenticode certificate (owner, 3.1), so SmartScreen stops warning; until then the release notes keep the "More info → Run anyway" steps.
- **Accessibility**: Narrator pass (same scope as VoiceOver) by the owner; **High Contrast**: `forced-colors` CSS so borders, focus and progress stay visible in Windows contrast themes.
- **Beta ends** when the owner has run the installer, a download, the extension hand-off, an update and an uninstall on a real PC ([build stages](stages.md), Release).

### 2.3 Shared (`shared/`)

- The `Platform` trait (2.0) and one progress/notification feed from the manager, so both apps show the same numbers.
- New Settings: **Keep the computer awake while downloading** (on), **Notify when a download finishes** (on), Dock/taskbar progress (on).
- New text in `src/locales/{en,hi}.json` with the Hindi style guide; menu text too; the language-file check runs.
- The Chrome extension stays one product, released with both apps under the same version.

## 3. Decisions for the owner

1. **Paid signing**: an Apple Developer ID (USD 99 a year) for signing and notarisation on macOS, and/or a Windows code-signing certificate (or Azure Trusted Signing) for Windows. Without them both apps keep working but warn on first open.
2. **Notifications**: `tauri-plugin-notification` (new dependency, both OSes), or each OS's own API from Rust (no dependency on macOS through the Objective-C runtime; WinRT toasts on Windows need COM calls through `windows-sys` or the `windows` crate, a larger new dependency).
3. **Builds**: macOS as one universal dmg (one download, about 100 MB larger) or two (today); add a Windows **ARM64** build.
4. **Deno (78–94 MB per architecture)**: keep it bundled (works offline, today), or download it the first time a video needs it (smaller app, but a download the user did not start; the suite promise allows only user-started network use, so this needs the owner's OK and a note in the first-run notice).
5. **WebView2 on Windows**: embed the offline installer (about 130 MB larger installer, nothing to fetch) or keep the download bootstrapper (Windows 11 and updated Windows 10 already have WebView2).
6. **Order**: macOS first (it can be tested on this Mac), or both in parallel (Windows only through CI and the owner's hand checks).

## 4. Build order and testing

Each step is one or more commits on `dev`, tested before the next. Nothing is pushed or tagged without the owner's OK.

| Step | What | Where | Tested on |
|---|---|---|---|
| 1 | Layout: `shared/`, `Platform` trait, per-OS config files | repository layout, workflows, scripts | Every check in [CLAUDE.md](../CLAUDE.md) "Commands"; both CI builds |
| 2 | macOS: menu bar, Dock progress and badge, keep awake, Where from, Downloads bounce, drag and drop, (notifications) | `desktop/mac/`, `shared/` | This Mac: the app, a real download, sleep held (`pmset -g assertions`), Finder Get Info |
| 3 | Windows: Mica, taskbar progress and overlay, keep awake, jump list, Edge host, MotW details, High Contrast, (notifications, ARM64) | `desktop/windows/`, `shared/` | CI build and tests; the owner's hand check on a PC |
| 4 | VoiceOver (here) and Narrator (owner) passes; startup measured on both | both | Recorded in `docs/` |
| 5 | Release **v0.2.0** (both apps and the extension, one tag) | release workflow | Both apps update themselves from 0.1.x to 0.2.0 |

**Built so far (2026-11-25):**
- Step 1, part 1: the Tauri project is in `shared/`, with every workflow, script, guide and spec path updated; all local checks pass. Still to do in step 1: the `Platform` trait and the per-OS config files.

Windows cannot be run from this Mac, so step 3 is checked by its CI build and tests and by the owner on a real PC.
