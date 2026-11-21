# Stage 5 brief: continuing the run

Everything a fresh session needs to continue Stage 5 (Final) of ks-download-manager from this file alone. Read it fully, then `CLAUDE.md`, `specs/stages.md` and `docs/decisions-stage5.md`. Written 2026-11-21.

## 1. Run rules

- **Parts in order, in one go (owner, after the Part 4 review):** Parts 5, 6 and 7, then full testing, then `stage5-final-review.md`, without stopping between parts. The owner does not run `/clear` between parts.
- **At the start of each part,** read this brief and `docs/decisions-stage5.md` again, so nothing is lost if the session's context is compacted.
- **When a part is done:** run the **Done means** checks (below) and `graphify update .`, commit locally, add a short **"Part N is done"** section to `docs/decisions-stage5.md` with the commits and the check results, and continue with the next part.
- **Do not ask the owner questions.** When a decision is open, take the option closest to the specs and to this brief, and record it in `docs/decisions-stage5.md` with the reason and the alternative. If a part cannot be finished, finish what you can, mark the rest **Not done** there, and continue.
- **Stop only** if continuing would risk losing the owner's data or breaking a repository, and then say exactly what stopped you.
- **Commits:** ks-download-manager and the workspace repo: local only, never push. One or more commits per part, Conventional Commits.
- **ks-utility may be pushed** (owner, after the Part 4 review): only the `main` branch and new version tags. Never force-push, never move or delete a tag, never push any other repo. Record each push in `docs/decisions-stage5.md`. No more stopping to ask for a push before re-pinning.
- **The history rewrite stays deferred** (section 6): do not start it.
- **Finish** by telling the owner to paste into Claude Cowork: "Follow the instructions in stage5-final-review.md." **No Claude attribution:** no `Co-Authored-By: Claude`, no `Claude-Session:`, no "Generated with Claude Code" lines. The owner is the only author.
- **Never commit** `stage5-plan-review.md` (untracked, kept as the style example for the final review file), `src-tauri/binaries/` or `extension/dist/` (both git-ignored).
- **Pre-approved:** the yt-dlp, ffmpeg/ffprobe and Deno binaries; tokio features; Zustand and TanStack Table/Virtual **only if** justified in the decisions log; GnuPG (Homebrew) as a development tool. **Anything else third-party: do not add it.** Write it ourselves (generic code in ks-utility), or leave the item undone and list it.
- **Rules for the whole stage:**
  - [Free Download Manager](https://www.freedownloadmanager.org/) is the reference for feature scope and overall feel.
  - The clean room is unchanged ([specs/clean-room.md](../specs/clean-room.md), [specs/ui.md](../specs/ui.md#references)): our own UI and icons, no code, assets, screenshots or wording from any download manager. Extension site rules come from observing the pages, never from another extension's code.
  - No hard-coded interface text anywhere once Part 4 lands: every string goes behind a key. New text added in Parts 3 and 5–7 needs both languages.
  - Out of scope: torrents.
- **Generic code goes to ks-utility** (`../ks-utility`, its own repo): commit there, tag a new version, push `main` and the new tag, then move the pin here in its own commit (normal fetch: `npm install`, `cargo check`, both lock files name the tag's commit).
- **No code moves during Stage 5:** do not move code into `shared/` or the platform folders (`desktop/mac/`, `desktop/windows/`, …). That is a separate, later stage (owner, Part 3 review).

### Done means (every part)
- `npm run build` succeeds.
- In `src-tauri/`: `cargo test -q`, and `cargo clippy -q --all-targets -- -D warnings` with no output.
- `npm run test:extension` and `npm run build:extension` succeed, and from Part 4 on `npm run test:lang` (the language-file check).
- Any new dependency or binary is in `docs/THIRD_PARTY.md` in the same commit. Specs changed → a line in `specs/log.md`; new spec → listed in `specs/index.md`.

### Practical notes
- **Never use `cd` in a command** (owner): a setting on this machine blocks reads outside the working directories, so every `cd` needs the owner's manual approval. Run everything from the repository root with paths relative to it, and use each tool's own folder option: `cargo … --manifest-path src-tauri/Cargo.toml` (`-p engine` for the engine), `npm --prefix <folder> …`, `git -C <folder> …`. For ks-utility, use its full path with the same options (`git -C /Users/…/KS-Suite/ks-utility`, `npm --prefix /Users/…/KS-Suite/ks-utility test`).
- Prefix cargo with `export PATH="$HOME/.cargo/bin:$PATH"`.
- `npm run sidecars` fetches yt-dlp and Deno, builds ffmpeg (already done on this Mac, cached in `src-tauri/binaries/`); `cargo build` copies them next to `target/debug/`.
- Real end-to-end tests without touching the owner's data: run the debug app as `HOME=<scratch>/home target/debug/ks-download-manager`, then send Chrome-style framed messages to `target/debug/ks-download-manager chrome-extension://ikplfdbdfikcokpegkapjdedlgifmkbe/` (a 4-byte native-order length, then JSON). With a long scratch path, the socket falls back to `/tmp/ksdm-<uid>/ext.sock`; remove it afterwards. The manager also has an ignored network test: `cargo test real_video -- --ignored`.
- A privacy hook refuses Bash commands that put this repo's path (it contains the owner's email address) next to a client identity such as a curl `-A` user agent. Run such commands from the current directory without the path, and use no `-A`.
- Release bundle: `npm run bundle:mac` (Tauri `.app`, then `scripts/sign-macos.sh`, ad-hoc unless an identity is given).

## 2. Done so far (branch `dev`, nothing pushed)

| Commit | What |
|---|---|
| `3209888` | docs: open Stages 5–10 merged into Stage 5 (Final); FDM as the scope reference; clean-room note for extension site rules |
| `42dca38` | Part 1: single-video downloads through bundled yt-dlp (unpacked build), LGPL ffmpeg we build (arm64 and x86_64), Deno; hidden temp folder beside the destination; final name reserved after yt-dlp exits; process-group stop; Update video component; ad-hoc hardened-runtime signing (`scripts/sign-macos.sh`) |
| `3fddd99` | chore: ks-utility pinned by tag `v0.1.0` (= commit `f772fa8`) in `package.json` and both `Cargo.toml`; both lock files resolve to `f772fa8` |
| `cd2721f` | Part 2: Chrome extension in `extension/`; the app as its own native messaging host restricted to extension ID `ikplfdbdfikcokpegkapjdedlgifmkbe`; socket as single-instance lock; engine fix for servers that ignore `If-Range`; file extension from Content-Type |
| `1c8e5cf` | Part 1 review fixes: temp-folder ownership marker (`src-tauri/src/marker.rs`), IDs never reused (`nextId` in `downloads.json`), GPG checks recorded, Deno licence texts (`scripts/deno-notices.mjs`), message and naming fixes, updater exempt from quarantine marking (decision recorded) |

Other local, unpushed commits:
- ks-utility `a1d12b9`: docs: pin by version tag, versioning rules (`specs/usage.md`, rules, overview, README, CLAUDE.md).
- KS-Suite workspace repo `6e17e8b`: docs: pin ks-utility by version tag, versioning rules (workspace `CLAUDE.md`, `specs/cross-repo.md`, `specs/log.md`).

**Part 4 status:** Part 3 review fixes committed (`bc67b18`). ks-utility side of Part 4 committed (`6ea7fb4`) and tagged `v0.2.0` locally; workspace `CLAUDE.md` updated (`0b37916`). Pinned to `v0.2.0` (`77a1cfb`). App-side Part 4 work committed. ks-utility `v0.2.1` (fix: the language-file command inside `node_modules`, plus new Hindi terms) committed and tagged locally. Owner pushed `v0.2.1`; pinned in `b4c4908`; `npm run test:lang` wired. **Part 4 is done.** After the Part 4 review: owner's Hindi wording (ks-utility `v0.2.2`) and the crash-test fix, then Part 5 (playlists). Decisions so far: `docs/decisions-stage5.md`, "Part 4".

Part 1 is "approved with changes" (done in `1c8e5cf`). Part 2 is "approved with changes" (done in `c1afa7c`). Part 3 (`ef4373a`, `7a7b9af`, `4956ff8`) is "approved with small changes": reservation markers, per-item leftover removal, the video speed-limit note (recorded in the decisions log).

## 3a. Part 2 review fixes (do first, as their own commit, before continuing Part 3)

Commit the uncommitted Part 3 files separately or leave them uncommitted, so they do not mix with this commit (for example `git stash push -u -- src-tauri/engine/src/limits.rs src-tauri/engine/src/ranges.rs`, then `git stash pop` after committing). Message, for example: `fix: Part 2 review: validators after dropping If-Range, safer hand-off`.

- **a. Engine, after If-Range is dropped:** once If-Range has been dropped for a download (`Ctx::whole_file_reply` sets `if_range` to `None`), every 206 reply must carry a validator equal to ours. A 206 with no validator, or a different one, is treated as "file changed" (`EngineError::FileChanged`: restart once, as before). While If-Range is still sent, behaviour is unchanged (today `Ctx::check_206` only compares when the reply has a validator; make the missing-validator case fail only after the drop). Tests (with the `if_range_unsupported` test-server option, plus a new option to leave out the ETag on 206 replies):
  - a server that ignores If-Range and sends 206 without a validator does not produce a mixed file;
  - one that sends matching validators completes.
- **b. Browser-download hand-off:** never take over a download Chrome marks as dangerous (`DownloadItem.danger` other than `safe`/`accepted`, or any danger state still being decided; add `danger` to `extension/src/chrome.d.ts`). The option's description, in both languages, must say three things:
  - Chrome's download safety check does not apply to handed-over files;
  - downloads that need a sign-in stay in Chrome;
  - single-use links may fail.
- **c. Fallback socket folder under `/tmp`:** on both the app side (`handoff::bind`) and the host side (`handoff::socket_path` / `ask`), use `/tmp/ksdm-<uid>` only if it is a real folder (not a link), owned by the current user, with mode `0700`; otherwise refuse. Test it.
- **d. Image button** (`extension/src/hover.ts`): do the look-up under the pointer (`elementsFromPoint`) at most once per animation frame.
- **e. App side of the socket** (`handoff::serve`): close a connection that sends no complete line within a few seconds. Test it.

Update `specs/extension.md` (and `specs/engine.md` for a) and `specs/log.md`, and add the review entry to `docs/decisions-stage5.md`.

## 3. Uncommitted Part 3 work (in the working tree)

- `src-tauri/engine/src/limits.rs` (new, not yet in `lib.rs`): `ConnectionBudget` (a total across downloads, a counter with `Notify`, our own `Permit` returned on drop, `set()` to change the total), `SpeedLimit` (a token bucket holding a quarter second, debt paid by waiting; `set(Option<bytes/s>)`), `HostLimits` (host → working connection count, for the session), `Adaptive { initial: 4, hosts }`, with unit tests.
- `src-tauri/engine/src/ranges.rs`: `Table::with_slots` (room for N workers, none started), `join()`, `active()`, `has_work_for_another()`, with a test.
- **Planned next, not written:**
  - `DownloadOptions` gains `adaptive: Option<Adaptive>`, `budget: Option<ConnectionBudget>`, `speed_limit: Option<SpeedLimit>` (all `None` by default, so existing tests keep their exact connection counts).
  - In `segmented()`: cap = min(connections, remaining, size / 512 KiB, at least 1). Start = the host's remembered limit, else `initial`, clamped to the cap. The first split is into `start` ranges.
  - `run_workers` starts `start` workers. The first waits for a budget permit; the others take one only if free. Tasks return their ID, and a `live` list tracks which slots run.
  - A 1 s controller tick:
    - Trouble since the last tick (429, 503, reset, timeout; workers count it in `ctx.trouble`) lowers the good count to what is running.
    - After 5 s without trouble it adds one worker per tick: below the good count (recovery after a brief network problem), or while the speed rose at least 5% since the last addition (a plateau stops this).
    - It only adds when there is work and a budget permit.
    - When the download ends, it remembers the good count for the host.
  - A worker that gets 429 or 503 while others are active retires at once (gives its range back).
  - Speed limit: `take(n)` after every chunk, in `fetch` and in `single`'s `stream_to`.
  - Tests against the throttled test server (`engine/tests/common/mod.rs`): the count ramps up; it backs off with `max_connections` and the host limit stays at or below it; it recovers after `fail_first`; a tiny file uses 1 connection; the speed limit paces; a budget of 1 means 1 connection.

## 4. Remaining parts

### Part 3: scheduler, queues, categories, settings (+ leftover-files cleanup)
- Global caps on total connections and on active downloads; an optional speed limit; queued downloads beyond the cap wait their turn. Planned: a new `Waiting` status. Resume or Download becomes "wants to run"; it starts when a slot is free, in queue order, then by date added. Video downloads count toward the active-download cap. For the speed limit, yt-dlp gets `--limit-rate` (an approximation: record it).
- **Adaptive connection count** (section 3): start with a few, add while speed rises, back off on 429, 503, resets and timeouts, remember the working limit per host for the session, and add connections back after a brief network problem (today, retired connections never return). Small files must get few connections; a 9 KB image used 16 in testing, and Wikimedia answered 429.
- **Categories** with save folders (Video, Audio, Images, Other). The Add dialog and extension hand-offs default to the category's folder.
- **Queues** in the sidebar: create, rename, delete (its downloads move to the main queue), start and stop a queue, move a download to a queue.
- **Settings screen**:
  - limits (active downloads, total connections, connections per download, speed limit);
  - category folders;
  - **Prefer MP4** (wire `Tools::set_prefer_mp4`; it already never lowers resolution);
  - **Update video component** (also stays on the failed row);
  - **Leftover download files** (below).

  Saved to `settings.json` in app data with `write_atomic`.
- **Folder picker** without new crates: plan `osascript` (`choose folder`, prompt and default folder passed as `argv`, never spliced into the script). A Windows equivalent is written but marked untested.
- **Zustand:** add only if justified; the hand-written store pattern may suffice (record either way).
- Update the stale "Stage 5" code comments in `src/features/downloads/types.ts` and `src/components/layout/Toolbar.tsx`.
- **Leftover download files (owner addition; closes the "orphan temp folder" item in the decisions log):**
  - Record ownership when a temp folder or a `.ksdownload` file is created. Video temp folders already carry `ks-owner.json` (`marker.rs`: app identifier, download ID, URL, creation time). For engine downloads, the `.ksdm` range map identifies the file; where there is no map, add an equivalent marker beside it (for example `<file>.ksowner`).
  - At start-up, off the critical path, look **only** in folders the app knows: the default save folder, the folders of downloads in the list, and the category folders from Settings. Find our temp folders and `.ksdownload` files that belong to no download in the list. Never scan anywhere else; never follow links out of those folders.
  - Never delete automatically. In Settings, show **Leftover download files** with the count and total size, a list of what was found, and a **Remove** button with a confirmation. Nothing is listed unless it carries our marker or a valid `.ksdm` map.
  - An item is removed only if its marker or map is still valid at the moment of deletion.
  - Tests:
    - a marked orphan is found, and removed after confirmation;
    - an unmarked folder with a matching name is never listed or touched;
    - an item belonging to a download in the list is never listed;
    - a link pointing outside the folder is not followed.
  - Update `specs/ui.md`, `specs/video-playlists.md` and `specs/log.md`, and both languages for the new text.

### Part 4: Hindi alongside English (language support in ks-utility)
Language support lives in ks-utility, as a utility every KS-Suite app can use to offer English and Hindi.

**In ks-utility**
- One translation file format for the whole suite (keys, placeholders, plural forms), documented in `specs/`.
- A shared catalogue of common interface text in English and Hindi (OK, Cancel, Remove, Delete, Settings, Pause, Resume, common errors and confirmations). App-specific text stays in each app's repo.
- TypeScript: the translation helper (lookup, placeholders, plurals, fallback to English when a key is missing), with tests. Plurals through the built-in `Intl.PluralRules`.
- Kotlin: no parallel runtime. A script that converts the shared files into Android string resources (`values/` and `values-hi/`), with tests.
- Rust: a small helper for message codes with parameters, so Rust never builds display sentences.
- The language rule in one place: follow the system, allow a choice of English or Hindi, remember it.
- The Hindi style guide as `specs/hindi-style.md`, plus the term list as a data file with disallowed alternatives. The spec table and the data must not drift (check it).
- A check any suite app can run on its language files, as one command usable from a TypeScript app and from a Kotlin build, documented in `specs/usage.md`. It fails when:
  - a key exists in one language and not the other;
  - a placeholder is missing, repeated or changed in the Hindi string;
  - a Hindi string uses a disallowed alternative;
  - a Hindi string contains Devanagari digits.
- No third-party library for any of this.
- Record in ks-utility's specs and in the workspace `CLAUDE.md` that every KS-Suite app supports English and Hindi through ks-utility.
- Also move the generic helpers written in the app meanwhile: `files::extension_for` / `name_with_type` (`src-tauri/src/files.rs`) and the `SpeedLimit` token bucket's arithmetic (the engine keeps the async waiting). Make `formatBytes`/`formatSpeed`/`formatTimeLeft` locale-aware with built-in `Intl`.
- Commit in ks-utility, **tag that commit `v0.2.0` locally** (MINOR: new helpers), do not push, and pin the app to `v0.2.0` (`package.json`, `src-tauri/Cargo.toml`, `src-tauri/engine/Cargo.toml`, both lock files), in its own commit.
- **No workaround for the unpushed tag:** no changes to Cargo's git database, no manual copy into `node_modules`, no offline build. Commit in ks-utility, tag `v0.2.0` locally, then **stop and ask the owner to push ks-utility and the tag** (give the exact commands). Pin the app to `v0.2.0` only after the owner confirms, using a normal fetch from GitHub (`npm install`, `cargo check`; check that both lock files name the tag's commit), in its own commit. The app-side Part 4 work that needs the new ks-utility code continues after that.

**Hindi style guide (use for every Hindi string)**
Natural app Hindi with familiar Devanagari transliterations, no stiff or Sanskritised words.

| English | Hindi |
|---|---|
| Settings | सेटिंग्स |
| Account / Profile | अकाउंट / प्रोफ़ाइल |
| Backup & Restore | बैकअप और रीस्टोर |
| Update | अपडेट |
| Download / Upload | डाउनलोड / अपलोड |
| Login / Sign out | लॉगिन / लॉग आउट |
| Password / OTP | पासवर्ड / ओटीपी |
| Search | खोजें |
| Save | सेव करें |
| Cancel | रद्द करें (not कैंसल) |
| Remove (from a list) | हटाएं |
| Delete (permanently) | डिलीट करें |
| Retry | फिर से कोशिश करें |
| Loading... | लोड हो रहा है... |
| Network / Offline | नेटवर्क / ऑफ़लाइन |
| Sync | सिंक करें |
| Subtitles / Audio | सबटाइटल / ऑडियो |
| Playback Speed | प्लेबैक स्पीड |
| Pause / Resume | रोकें / फिर शुरू करें |
| Open / Open folder | खोलें / फ़ोल्डर खोलें |
| File / Folder / Link | फ़ाइल / फ़ोल्डर / लिंक |
| Paste | पेस्ट करें |
| Queue | क्यू |
| Speed / Time left | स्पीड / बचा समय |
| Finished / Failed | पूरा हुआ / नहीं हो पाया |
| Video / Audio / Images | वीडियो / ऑडियो / इमेज |
| Category | कैटेगरी |

Tone and formatting:
- Polite, clear action verbs (चुनें, दर्ज करें, आगे बढ़ें).
- Button and tab text brief, so it fits compact UI slots.
- Latin digits (0-9) and symbols (₹, %) stay as they are; never Devanagari digits.
- One term per concept everywhere. For a term not in the list, choose in the same style, add it to the guide and the data file, and mark it for the owner's review.
- Spell consistently with the guide: फ़ाइल, फ़ोल्डर, हटाएं, खोलें.

**In ks-download-manager**
- All interface text in `en` and `hi` language files, looked up by key. Common words come from the ks-utility catalogue; app-specific text stays here.
- Rust sends message codes with parameters instead of finished English sentences (today `Record.error` is an English sentence and `Record.error_code` a hint; the extension already shows codes through `_locales`).
- The language follows the system, with a remembered English/Hindi setting. Sizes, numbers and dates use built-in `Intl` formatting.
- Extension labels in both languages through Chrome's `_locales` (`extension/_locales/en/messages.json` exists; add `hi`).
- The app runs the ks-utility check as part of its tests (including the extension's `_locales`).
- `docs/hindi-review.md`: every string as key, English, Hindi, covering both shared and app-specific text. It starts with the glossary and marks any string you were unsure about.

### Part 5: playlists
- The playlist picker per [specs/video-playlists.md](../specs/video-playlists.md), on the same sidecars:
  - every entry selected when the list opens, a **Select all** box with a count, shift-click for ranges;
  - title, duration and thumbnail per entry;
  - each entry its own row grouped under the playlist name, saved in a folder named after the playlist, with an optional number prefix;
  - pause, resume and retry per entry and per playlist;
  - unavailable entries marked and skipped.
- `video::Tools::info(url, true)` already reads playlists (`--flat-playlist`) into `PlaylistEntry`; today the manager refuses playlist links with a message.
- Thumbnails: keep the CSP tight (`img-src 'self' data:`); fetch them through Rust and pass data URLs (record the decision).
- Reconcile with the owner's single-video rule ("one Download action, no quality picker"): the spec's per-download quality choice and audio-only option are open; take the closest reading and record it.

### Part 6: benchmarks
- A repeatable benchmark against the throttled test server (1, 4, 8, 16, 20, 32 connections), results in `docs/benchmarks.md`, a regression check ready for CI.
- No speed claim in the README or the app unless the numbers support it. A per-connection-throttled local server measures how the engine scales, not real-world speed; say so wherever a number is quoted.
- The benchmark measures after the adaptive ramp-up, or with adaptive connections off, and says which (owner, Part 3 review).

### Part 7: polish
- First-run notice (the user's responsibility, no DRM bypass), full keyboard reachability, contrast, reduce-motion.
- The 10,000-row scroll test with a virtualised list (TanStack only if justified; fixed-height rows make our own windowing simple).
- Window material (Tauri's built-in window effects; watch the contrast).
- Clipboard pre-fill in Add download (no new crate; check whether macOS prompts on reading the clipboard, and record it).
- Start-up restore of the download list off the critical path.
- Mark downloaded files as coming from the internet: the quarantine attribute on macOS; the Windows Mark of the Web (`Zone.Identifier`) written but marked untested. This includes files handed over by the extension (video, image and browser downloads), not only those added in the app. **The yt-dlp update must stay unmarked**, as already decided and recorded in `specs/video-playlists.md` ("Updates and 'downloaded from the internet' marking") and `src-tauri/src/component.rs`.

### Later, not in Stage 5
- Release: Windows build, signing through `SIGNING_*` secrets in CI, notarisation, tag-driven versions, the release workflow, DMG after re-signing.
- What needs a real Developer ID certificate is in the decisions log (Part 1): library validation without the ad-hoc exception, notarisation of the malformed framework copy, Deno with `allow-jit`.

## 5. Final steps (after Part 7)

1. **Full testing:** run everything:
   - `npm run build`;
   - `cargo test` and `cargo clippy --all-targets -- -D warnings` in `src-tauri`;
   - the extension build and tests;
   - the ks-utility tests (Rust, TypeScript, the Kotlin conversion script, the language check);
   - the benchmark.
2. **Real downloads**, where possible: a direct file with pause and resume, a YouTube video, an X image through the extension hand-off, a playlist. Record exactly what you ran and what you could not run, fix what fails, and re-run.
3. **Write `stage5-final-review.md`** in the repository root: a review request for Claude Cowork, in the same style as `stage5-plan-review.md` (review only, no edits, findings ordered by severity). It must contain:
   - the commit list per part, with what each commit does;
   - every test and check with its result, and a separate list of what was **not** tested and why;
   - the contents of `docs/decisions-stage5.md`, or a pointer to it;
   - every deviation from the specs or from the owner's prompts;
   - every new dependency and binary, with its licence and how it is verified;
   - the security-sensitive areas to look at first: the native-messaging hand-off, how yt-dlp, ffmpeg and Deno are run, file naming and deletion (including the leftover-files cleanup), and the extension's permissions;
   - what the owner must test by hand, as a numbered checklist;
   - instructions for the reviewer.
4. Finish by telling the owner to paste this into Claude Cowork: "Follow the instructions in stage5-final-review.md."

## 6. Deferred: history rewrite (do not start yet)

The owner asked to remove Claude from the git history of three repos: ks-utility, ks-download-manager, and the KS-Suite workspace repo (the parent folder `..`, done last). **Check first, and stop if any of these is false:**
- the Stage 5 run is finished and the owner has approved it;
- all three working trees are clean, with nothing uncommitted or stashed;
- all three repos are fully pushed, including tags;
- no other Claude Code session is open in any of them.

Then, in this order:
1. Back up: `git clone --mirror` of each of the three repos into a new folder outside all of them. Tell the owner the paths; never delete them.
2. ks-utility first. Rewrite every commit message on every branch and tag to remove only:
   - any `Co-Authored-By:` line naming Claude;
   - any `Claude-Session:` line;
   - any "Generated with Claude Code" line.

   Keep subjects, bodies, authors and dates unchanged. Use built-in git only (no tool installs). Keep the tags (`v0.1.0`, `v0.2.0` and any others) on the matching rewritten commits.
3. Verify ks-utility:
   - the commit count per branch is unchanged;
   - the tree hash at each tag is identical to before;
   - a search of every commit message finds none of the three kinds of line;
   - its tests pass.
4. ks-download-manager: the same rewrite and the same verification.
4b. The KS-Suite workspace repo, last: the same rewrite and the same verification (commit count per branch, tree hash at each tag if any, message search). It has no build of its own; check that its specs and `CLAUDE.md` are unchanged by comparing tree hashes.
5. Re-pin: the tag names are unchanged but the commit IDs behind them are new. Refresh both lock files to the new commit for the pinned tag, and update any commit ID written in `docs/THIRD_PARTY.md`, `CLAUDE.md` or the specs. This depends on ks-utility being pushed, so **stop here and wait for the owner**.
6. Record in ks-utility's `specs/usage.md` that tags were moved once, on that date, to remove tool attribution from history: the single exception to the rule that tags never move.

Push nothing. Give the owner the exact push commands for each of the three repos, and a summary: commits rewritten per repo, the old and new ID of each tag, and the verification results.

Commits with a `Co-Authored-By: Claude` line in this repo on 2026-11-21: 25 of 29, from the Stage 1–4 history up to `3209888` (`git log --grep='Co-Authored-By: Claude' -i`). Commits from `42dca38` on have none. Check ks-utility the same way, and also look for the other two kinds of line.
