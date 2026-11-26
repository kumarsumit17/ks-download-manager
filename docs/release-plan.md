# Release plan: v0.1.0

Status: **approved by the owner on 2026-11-21 (phase A being built).** Answers and additions B–E are in section 8. Branch `release-setup`, local commits only; the owner pushes. Decisions taken while building go in `docs/decisions-release.md`.

Goal: a first public release, v0.1.0, built by GitHub Actions from a tag, as a **draft** GitHub release with a macOS `.dmg` per architecture, the Chrome extension `.zip`, `SHA256SUMS` and build-provenance attestations; a Windows NSIS `.exe` installer (beta) joins the first release it is ready for.

Read before writing this: `CLAUDE.md`, `specs/stages.md` ("Release"), `docs/decisions-stage5.md` (Part 1: signing), `docs/stage5-brief.md` ("Later, not in Stage 5"), and the Windows code paths in `shared/src-tauri/src/` (`handoff.rs`, `video.rs`, `manager.rs`, `mark.rs`, `picker.rs`, `clipboard.rs`, `component.rs`, `open.rs`).

## 0. Summary and my estimate

- **macOS can ship as v0.1.0.** Everything except Developer ID signing has already run on this Mac (Part 1 and the final testing). The new work is CI, a DMG made *after* re-signing, and the signed/unsigned switch.
- **macOS is not blocked by Windows** (owner, F). The macOS and extension release is built first (section 7, phase A); the work then stops for the owner's history rewrite and push. Windows is added afterwards to the same two workflows (phase B) and **joins the first release it is ready for, as beta**, whether that is v0.1.0 or a later tag.
- **Windows ships as "beta, untested by hand", never as equal to macOS.** Tags stay `vX.Y.Z` (the suite rule; a `v0.1.0-beta` tag fails the version check by design). The Windows file is named `…-windows-x64-setup-beta.exe`, and the release notes and README say "beta, untested by hand". Reasons: no person has run the app on Windows; CI can run unit and integration tests on `windows-2025`, but not the installer, SmartScreen, Chrome's native-messaging launch, the folder picker dialog, or a real YouTube download (datacenter IPs are often blocked). The "-beta" label is dropped only in a later release, after the Windows hand checks (section 9) have passed on a real PC.
- Until every Windows test in section 4 passes on `windows-2025`, the release workflow does not build Windows at all (a job switch, off by default); a release is never held back for it.
- **No release is published until the hand checks in section 9 pass on the built files of that draft** (owner, G).

## 1. Versions from the tag

`shared/scripts/set-version.mjs <tag>` (Node, no packages):

- Accepts only `^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$`. Anything else (`v0.1`, `0.1.0`, `v0.1.0-beta`, `v01.0.0`) exits 1 with a message, so the build fails before anything is compiled.
- Writes `X.Y.Z` into `package.json` (adds `version`), `shared/src-tauri/Cargo.toml` (`[package] version`), `shared/src-tauri/Cargo.lock` (only the `ks-download-manager` package entry, so the lock file still matches and `cargo` does not rewrite it), `shared/src-tauri/tauri.conf.json` (adds `version`) and `shared/extension/manifest.json`.
- Edits are textual and checked: each file must contain exactly one placeholder `0.0.0` (or, for the two JSON files without a version, no `version` key) or the script fails. Run only in CI on the checkout; the repository keeps `0.0.0`.
- Tests: `shared/scripts/test/set-version.test.mjs` (`node --test`), on a copy of the four files in a temp folder: good tag, each bad tag form, a file already changed.

## 2. CI workflow: `.github/workflows/ci.yml`

Phase A has the macOS row of the matrix and `bench`; `ffmpeg-windows` and `windows-2025` are added in phase B (section 7). Runners are named image versions, never `-latest` (owner, D; reasons in `docs/decisions-release.md`).

Triggers: `push` (every branch) and `pull_request`. `permissions: contents: read`. `concurrency` cancels an older run of the same ref.

Jobs:

| Job | Runner | Steps |
|---|---|---|
| `ffmpeg-windows` | `ubuntu-24.04` | Cross-build ffmpeg/ffprobe for Windows (section 4.1). Cached. |
| `check` (matrix) | `macos-15`, `windows-2025` | `npm ci`, `node shared/scripts/sidecars.mjs <triple>` (Windows: takes the ffmpeg artifact), `npm run build`, `npm run test:lang`, `npm run test:ui`, `npm run test:extension`, `npm run build:extension`, `node --test shared/scripts/test/`, `cargo test --locked`, `cargo clippy --all-targets --locked -- -D warnings` |
| `bench` | `macos-15` | `npm run bench` (local server; no internet download) |

- Rust: no setup action. The runners have `rustup`; `rust-toolchain.toml` makes it install Rust 1.100 with clippy on first use.
- Caches (`actions/cache`): `~/.npm` by `package-lock.json`; `shared/src-tauri/target` and `~/.cargo/registry` by `Cargo.lock` + OS; `shared/src-tauri/binaries/.cache` by `sidecars.json`; the built ffmpeg by **the source hash in `sidecars.json` + the hash of the build script + the target triple** (a change to the configure line rebuilds it).
- `sidecars.mjs` is required by `cargo test`/`build` only through `externalBin`; the check job runs it so `tauri build`-time paths exist and the hash checks are exercised on both OSes.
- The macOS `check` job builds ffmpeg itself (native, ~4 min uncached), cached the same way.

## 3. Release workflow: `.github/workflows/release.yml`

Trigger: `push` of tags `v*` only. Top level `permissions: {}`; build jobs get `contents: read`; only the final `release` job gets `contents: write`.

```
version ─┬─ mac (aarch64) ──┐
         ├─ mac (x86_64) ───┤
         ├─ extension ──────┼── attest ── release (draft, SHA256SUMS)
         └─ ffmpeg-windows ─ windows ─┘   (phase B; off until section 4 passes)
```

- **Build provenance** (owner, H): an `attest` job runs GitHub's official `actions/attest-build-provenance` (pinned to a commit SHA) over every release file: both DMGs, the extension zip, the Windows installer when built, the ffmpeg source tarball and `SHA256SUMS`. That job alone has `id-token: write` and `attestations: write` (plus `contents: read`). The repository is public, so attestations are available without a paid plan. Anyone can check a file with `gh attestation verify <file> -R kumasumit/ks-download-manager`; `docs/release.md` and the README say how, and the release step itself runs that check on each file before creating the draft.

- `version`: runs `set-version.mjs "$GITHUB_REF_NAME"`; a bad tag fails here and nothing else runs. It also fails unless the tagged commit is on `main` (`git merge-base --is-ancestor "$GITHUB_SHA" origin/main`, owner, B). Every later job runs the script again on its own checkout.
- Each build job uploads its files with `actions/upload-artifact`; `release` downloads them, writes `SHA256SUMS` (`sha256sum` over every file, sorted), and creates the draft with the **`gh` CLI** that is preinstalled on the runners: `gh release create "$TAG" --draft --title … --notes-file notes.md <files>`.
- **tauri-action is not used.** Our macOS flow must re-sign after Tauri and make the DMG itself (below), and `gh` covers uploading; using `tauri build` directly keeps one less third-party action. Actions used: `actions/checkout`, `actions/setup-node`, `actions/cache`, `actions/upload-artifact`, `actions/download-artifact`, each pinned to a full commit SHA with the version in a comment.
- Release notes are generated from `CHANGELOG.md` (the section for this version) plus a block per platform that depends on whether signing secrets were present (section 3.1/3.2), so the text always matches the files.
- Re-running the workflow on the same tag: `release` deletes an existing **draft** with that tag first, never a published release (it fails instead).

### 3.1 macOS

- **Two `.dmg` files, not universal**, as already decided in Part 1: Deno alone is 80 MB (arm64) + 99 MB (x86_64), and a universal app would be about 430 MB. Files: `ks-download-manager-vX.Y.Z-macos-arm64.dmg`, `…-macos-x64.dmg`.
- Both are built on `macos-15` (Apple silicon). x86_64 is cross-compiled (`--target x86_64-apple-darwin`; ffmpeg already supports cross builds with `--disable-x86asm`). A smoke test runs the x86_64 sidecars with `-version` under Rosetta when the runner has it; otherwise it is a hand check.
- Steps per architecture: `npm run tauri build -- --target <triple> --bundles app` → `shared/scripts/sign-macos.sh <app> <identity>` → **DMG made after signing** with `hdiutil create` (a folder with the app and an `/Applications` link, UDZO) by a new `shared/scripts/make-dmg.sh` (system tools only). This fixes the open item "DMG after re-signing": Tauri's DMG would contain the app *before* our re-sign.
- **Secrets present** (`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`): the `.p12` is imported into a temporary keychain (deleted at the end of the job, also on failure), `sign-macos.sh` gets the identity, the app is zipped (`ditto`) and notarised (`xcrun notarytool submit --wait`), stapled, then the DMG is made, signed, notarised and stapled. Two notarisation rounds, so the app inside works offline too. Tauri's own signing is turned off for this (`signingIdentity` stays `-`; our script signs everything inside-out, then the app).
- **Secrets absent**: ad-hoc (`-`), exactly as on this Mac today. Notes say: macOS will say the app "cannot be opened" or is "damaged"; first open with **System Settings → Privacy & Security → Open Anyway** (macOS 15+), or `xattr -dr com.apple.quarantine "/Applications/KS Download Manager.app"`.
- The switch is one check in the workflow (`if [ -n "$APPLE_CERTIFICATE" ]`), so the same workflow works both ways without edits. A partial set of Apple secrets fails the job with a clear message rather than silently building unsigned.
- With a real certificate, `sign-macos.sh` gets the checks Part 1 left open: yt-dlp runs **without** `disable-library-validation` (the ad-hoc entitlement file is used only for `-`); if notarisation rejects `_internal/Python.framework/Python` (the malformed copy), the script deletes it before signing, as recorded. The notary log is printed on failure.

### 3.2 Windows

- `npm run tauri build -- --bundles nsis` with `bundle.windows.nsis.installMode: "currentUser"` (per-user, no admin) and `webviewInstallMode: downloadBootstrapper` (Windows 10/11 already have WebView2; the bootstrapper installs it per-user if not).
- File: `ks-download-manager-vX.Y.Z-windows-x64-setup-beta.exe` (section 0).
- **Unsigned** in the first Windows release; notes say SmartScreen will show "Windows protected your PC" → **More info → Run anyway**. No `.pfx` path is built (owner, I): new code-signing certificates can no longer be exported as `.pfx` files anyway.
- **Intended route after the first release: SignPath Foundation** (free code signing for open-source projects). It needs a public code-signing policy page; a draft is written now as `docs/code-signing-policy.md` (who may commit, review and approve releases; that only binaries built by this repository's GitHub Actions from a tag are signed; the privacy statement that the app connects only for downloads the user starts; the "Free code signing provided by SignPath.io, certificate by SignPath Foundation" line, marked as not yet true). Applying, and SignPath's own GitHub action (third-party, so it needs an exception to the "official actions only" rule), are owner decisions for later and are not built now. `docs/release.md` notes this.

### 3.3 Extension

- `npm run build:extension` after `set-version.mjs`, then `shared/extension/dist` zipped (files at the root of the zip, `manifest.json` at top level) as `ks-download-manager-extension-vX.Y.Z.zip`.
- A check in the job: the zipped `manifest.json` has the same `key` as the repository's, and the ID computed from it (SHA-256 of the DER key, first 32 hex digits mapped to `a`–`p`) is `ikplfdbdfikcokpegkapjdedlgifmkbe`; otherwise the job fails.

### 3.4 Licences in each installer

Unchanged mechanism: `shared/scripts/sidecars.mjs` writes `shared/src-tauri/binaries/licenses/` (yt-dlp `LICENSE` + `THIRD_PARTY_LICENSES.txt`, ffmpeg `COPYING.LGPLv2.1` + `LICENSE.md` + `SOURCE.txt`, Deno `LICENSE.md` + third-party notices) and Tauri bundles it as `licenses/` on both systems. A release step lists the bundle and fails if any of those files is missing. The ffmpeg source tarball is attached to the release (LGPL "source offer"), with its hash in `SHA256SUMS`; for Windows, `SOURCE.txt` also names the toolchain and configure line.
- **Deno notices, the 123 crates without a licence file** (owner, C): `shared/scripts/deno-notices.mjs` adds the standard text of each licence those crates declare (SPDX identifiers parsed from the `license` field; texts from SPDX's `license-list-data` repository at a pinned tag), once per licence, and lists which crates each text applies to. The notices file and `docs/THIRD_PARTY.md` say plainly that these texts come from the declared licence, not from the crate's own file. Crates that declare nothing stay listed as such.

## 4. Windows, made real and tested on `windows-2025`

### 4.1 Bundled programs for Windows

- **yt-dlp**: `yt-dlp_win.zip` (the unpacked "onedir" build, same reasoning as macOS: no temp unpacking per run), 2026.10.26, SHA-256 `fd51b212cf8380c7a52ee08c0d6be7f3c59b2f2c832eaf7b45acfae6eab57419` (from the release's `SHA2-256SUMS`; the macOS pin matches the same file, so the source is consistent). Unpacked with `tar` on Windows. `component.rs` already names this asset for updates.
- **Deno**: `deno-x86_64-pc-windows-msvc.zip`, 2.9.8, SHA-256 `cba84b26e0df5ae7f74fc66b86a95b866872a86292ebd84fa1e959927edd93bf` (from the release's `.sha256sum`).
- **ffmpeg/ffprobe: yes, from the same pinned tarball with the same rules, built in CI.** Cross-compiled on `ubuntu-24.04` with Ubuntu's `mingw-w64` and `nasm` packages: `--target-os=mingw32 --arch=x86_64 --cross-prefix=x86_64-w64-mingw32-`, with the same `--disable-autodetect --disable-gpl --disable-nonfree --enable-static --disable-shared`, `--enable-zlib` kept by building zlib first from a pinned, hash-checked source tarball (zlib licence; macOS uses the system copy, Windows has none), `--enable-schannel` (the Windows TLS, as Secure Transport on macOS), Windows' own threads (`w32threads`, so no winpthreads), and `-static-libgcc`. `shared/scripts/build-ffmpeg.sh` gains a `windows-x86_64` target; one script, one configure policy. The build ends with `ffmpeg -buildconf` checked for `--enable-gpl`/`--enable-nonfree` absence. Licence notes add the mingw-w64 runtime notice.
- Why not a prebuilt (BtbN `win64-lgpl`): its configure line enables many external libraries we do not need, it is rebuilt nightly (hard to pin reproducibly), and it would make Windows differ from macOS. Kept as the fallback if the cross build cannot be made to work; I would ask before switching.
- Why not MSYS2 on the Windows runner: the official MSYS2 setup action is third-party, and building through the preinstalled MSYS2 is slower; the Ubuntu cross build is faster and easier to cache.
- `sidecars.mjs` gets the Windows targets (`x86_64-pc-windows-msvc`), Windows file names (`.exe`), `tar` instead of `ditto` there, and `--ffmpeg-from <dir>` to take the prebuilt ffmpeg artifact instead of building (Windows runners cannot run the cross build).

### 4.2 Extension hand-off on Windows

- **Host registration**: an NSIS installer hook (`bundle.windows.nsis.installerHooks`, a `.nsh` file in `shared/src-tauri/windows/`) writes `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.ks.downloadmanager` (default value = path of a manifest JSON in the install folder, also written by the installer, `path` = the app exe, `allowed_origins` = our extension only). The uninstall hook deletes the key and the file. The app also rewrites both at start-up if they are missing or point elsewhere (with `reg.exe`, no crate), so a moved install heals itself, as on macOS.
- **Named pipe instead of the Unix socket**: `tokio::net::windows::named_pipe` (tokio's `net` feature, already enabled; no new crate). Pipe name `\\.\pipe\com.ks.downloadmanager-<user name>`; `first_pipe_instance(true)` makes it the single-instance lock, `reject_remote_clients(true)` refuses network clients. Same protocol (one line of JSON in, one out, same `validate`, same 5 s line limit, same 8 KB cap).
- **Same-user check (needs your approval):** the Unix code answers only connections from the same user ID. On Windows the default pipe permissions already deny write access to other users, but another user on the same PC could *create* a pipe with our name first and receive the URLs our host sends. To close that, both sides check that the other end's process belongs to the same Windows user (`GetNamedPipeServerProcessId` / `GetNamedPipeClientProcessId` + the token's user SID). That needs **`windows-sys`, already in `Cargo.lock`** (0.61.2, through tokio and Tauri): no new package in the lock file, only a new direct dependency with a few features (`Win32_System_Pipes`, `Win32_Security`, `Win32_System_Threading`, `Win32_Foundation`). It also needs a small amount of `unsafe` in one function. **Alternative** if you say no: rely on the default permissions and record the squatting risk as a known limitation of the Windows beta.
- **Starting the app from the host**: Chrome runs native hosts inside a job object, so a plain child would be closed with Chrome. The host starts the app with `CREATE_BREAKAWAY_FROM_JOB | DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`; if breakaway is refused, it falls back to `explorer.exe <app exe>` (starts it outside the job). Hand check: the app survives Chrome closing.
- **Single instance**: a second copy finds the pipe taken, asks the first to show its window (`Show`), and exits, as on macOS.
- Tests (run on `windows-2025`): pipe bind is a single-instance lock; a second bind reports `AlreadyRunning`; the host round trip (`serve_host` with a real pipe server); a connection without a full line is closed; the registry manifest JSON names only our extension; `reg.exe` write/read/delete of a test key under `HKCU\Software\KSDM-test-<random>`; the same-user check accepts our own process (if approved).

### 4.3 The other Windows paths, each with a test on Windows

| Path | What changes | Test on `windows-2025` |
|---|---|---|
| Process group stop (`video.rs`) | Kept: `CREATE_NEW_PROCESS_GROUP \| CREATE_NO_WINDOW`, then `taskkill /T /F /PID` | Starts a small tree (a test helper that spawns a child and sleeps), stops it, checks both PIDs are gone (`tasklist`) |
| `taskkill` | As above; also checks exit status | Same test; plus "already exited" is not an error |
| `attrib +h` (`manager.rs`) | Add `CREATE_NO_WINDOW` | Creates a folder, hides it, checks `FILE_ATTRIBUTE_HIDDEN` via `std::os::windows::fs::MetadataExt::file_attributes` |
| Mark of the Web (`mark.rs`) | Kept (writes `:Zone.Identifier`) | Writes the mark, reads the stream back: `ZoneId=3` and the URL fields |
| PowerShell folder picker (`picker.rs`) | Add `[Console]::OutputEncoding = UTF8` (Windows PowerShell 5.1 otherwise writes non-ASCII paths, e.g. Hindi folder names, in the OEM code page) and `CREATE_NO_WINDOW` | The dialog cannot be clicked in CI: a test runs the same script with a test switch that skips `ShowDialog` and prints `KSDM_START`, with a Hindi path, and checks it comes back unchanged. The dialog itself is a hand check |
| Clipboard read (`clipboard.rs`) | Same UTF-8 fix, `CREATE_NO_WINDOW` | `Set-Clipboard` with an English and a Hindi URL, then our `read()`; if the runner has no clipboard, the test reports "skipped: no clipboard" and it becomes a hand check (I will say so) |
| `certutil` / `tar` update path (`component.rs`) | `CREATE_NO_WINDOW` | Hashes a known file (expected hash), unpacks a small zip made in the test |
| `explorer.exe /select,` (`open.rs`) | Do not treat explorer's exit code 1 as failure (it returns 1 on success) | Unit test of the command built; opening is a hand check |

- **Every program the app starts on Windows gets `CREATE_NO_WINDOW`** through one small helper. Without it, a GUI-subsystem app flashes a console window for each `powershell`, `attrib`, `taskkill`, `certutil`, `tar`, yt-dlp, ffmpeg and Deno run.
- The data folder: Tauri's `app_data_dir` on Windows is `%APPDATA%\com.ks.downloadmanager`; `data_dir_without_tauri()` already matches. A test checks it against Tauri's path.
- File names: ks-utility's `file_name.rs` already handles Windows reserved names; a Windows-only test runs a video title with `:`, `?`, `CON`, a trailing dot and a long name through it and creates the file.
- Full `cargo test` and the engine tests run on Windows as part of CI, so everything else in the engine (resume, rename, the leftovers scan) gets a Windows run for the first time. Any failure there is fixed or, if it cannot be fixed, reported and Windows is left out.

## 5. macOS native-messaging host from `/Applications`

- The app writes `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.ks.downloadmanager.json` with `path` = its own executable. From `/Applications` that is `/Applications/KS Download Manager.app/Contents/MacOS/ks-download-manager`, which is correct.
- **Problem found while planning:** if the app is first opened straight from the mounted DMG (`/Volumes/…`) or from a quarantined copy in Downloads (Gatekeeper's App Translocation, `/private/var/folders/…/AppTranslocation/…`), the manifest points to a path that disappears. Fix: do not write the manifest when the executable is under `/Volumes/` or contains `/AppTranslocation/`; the next start from `/Applications` writes it. A unit test covers both paths.
- **App deleted**: the manifest file stays (a few hundred bytes). Chrome then fails to start the host, and the extension already shows its "app not found" message ("Specified native messaging host not found" / host exited are handled in `shared/extension/src/background.ts`). Nothing runs; harmless. macOS has no uninstaller hook, so it cannot be removed automatically; README tells how to delete it by hand.
- Hand check: install from the DMG to `/Applications`, open once, use the extension; then delete the app and confirm the extension says the app is not installed.

## 6. Docs

- `README.md`: the app does not check for updates by itself (no network use except the downloads the user starts); new versions are published on the repository's GitHub Releases page (owner, E).
- `README.md` install section: macOS (which DMG, drag to Applications, first open of an unsigned app), Windows (beta, SmartScreen, per-user install, uninstall from Settings → Apps), the extension (unzip, `chrome://extensions` → Developer mode → Load unpacked, and that it must stay in that folder), checking `SHA256SUMS` (`shasum -a 256 -c`, `Get-FileHash`).
- `CHANGELOG.md` starting at `## 0.1.0 — <date>` (Keep a Changelog layout), written from the stage specs, not invented.
- `docs/release.md` for the owner: each secret, where it comes from and how to add it (`gh secret set NAME < file`), the branch and tag steps (owner, B: `release-setup` is merged into `dev`, `dev` into `main`, and the tag is made on `main`; exact commands; the workflow refuses a tag whose commit is not on `main`), how to tag (`git tag -a vX.Y.Z -m …` then `git push origin vX.Y.Z`), what the Actions run looks like, how to check the draft (download, compare `SHA256SUMS`, `gh attestation verify`), how to publish, and how to retry a failed run. It states plainly: **the draft is not published until every hand check in section 9 has passed on the files attached to that draft** (not on a local build); a failed check means a fix and a new tag, never editing the draft's files by hand.
- `docs/code-signing-policy.md`: draft for SignPath Foundation (section 3.2), not linked from the README until signing is set up.
- `specs/stages.md` "Release" updated to what was built; `specs/log.md` line; `docs/THIRD_PARTY.md` gets the GitHub actions (with SHAs), the Windows yt-dlp/Deno/ffmpeg and the mingw-w64 runtime, and `windows-sys` if approved.

## 7. Order of work (one commit each, Conventional Commits, no attribution)

**Phase A: macOS and the extension (not blocked by Windows)**
1. `feat: set the version from the tag` (script + tests).
2. `fix: do not register the chrome host from a disk image or translocated copy` (section 5).
2b. `fix: standard licence texts for deno crates without a licence file` (section 3.4, owner C).
3. `ci: checks on macOS` (`ci.yml`: the `check` job on `macos-15` and `bench`) and `ci: tag-driven draft release for macOS and the extension` (`release.yml` with `version`, both mac jobs, `extension`, `attest`, `release`; `make-dmg.sh`, notes template), then `docs: release, install, changelog, code-signing policy draft` (README macOS + extension, `CHANGELOG.md`, `docs/release.md`, `docs/code-signing-policy.md`, specs).

**Then I stop** for the owner's history rewrite and push. I cannot run GitHub Actions from here: once the branch is pushed (not a tag), CI runs, and I fix what fails from `gh run view --log-failed`. The first tag can be made once CI is green.

**Phase B: Windows, on the same workflows**
4. `feat: windows sidecars` (pins, `sidecars.mjs`, `build-ffmpeg.sh` Windows target, `ffmpeg-windows` job).
5. `feat: windows hand-off over a named pipe` (+ registry, installer hooks, single instance).
6. `fix: windows process, file and dialog paths` (no-window helper, UTF-8, explorer exit code, tests).
7. `ci: windows checks` (`windows-2025` added to the `check` matrix) and, once all of section 4 passes there, `ci: windows beta installer in the release` (turns the Windows job on) + README Windows section.

I will not create a tag, push, or publish.

## 8. Owner's answers and additions (2026-11-21)

1. `windows-sys` as a direct dependency: **yes** (phase B). Recorded in `docs/THIRD_PARTY.md` when it is added.
2. Windows as beta: decided (F): beta, joins the first release it is ready for.
3. macOS v0.1.0 is **ad-hoc signed unless the Apple secrets exist when the tag is pushed**.

Additions:
- **B. Branches and tag:** `release-setup` → `dev` → `main`; the tag goes on `main`. The release workflow fails if the tagged commit is not on `main`. Exact steps in `docs/release.md`.
- **C. Deno notices:** standard text of each declared licence for the 123 crates without a licence file, stated as such (section 3.4).
- **D. Runner images:** named versions (`macos-15`, `windows-2025`, `ubuntu-24.04`) instead of `-latest`, with the reason in `docs/decisions-release.md`.
- **E. README:** the app does not check for updates; new versions are published on GitHub Releases.
- The history rewrite in `docs/stage5-brief.md` (section 6) now covers the `release-setup` branch as well as `dev`.

## 9. Hand checks I cannot do from this session

**macOS**
- The DMG opens, shows the app and the Applications link; drag-install works (both DMGs; the x86_64 one on an Intel Mac or under Rosetta).
- Unsigned build: Gatekeeper's first-open steps in the notes work exactly as written (macOS 15 and later changed them).
- Signed build (when secrets exist): opens without any warning, also offline; `spctl -a -vv` says "Notarized Developer ID"; yt-dlp works without the ad-hoc exception; a YouTube download (Deno with JIT) succeeds from the installed app.
- Extension hand-off from `/Applications`, and the "app deleted" behaviour (section 5).

**Windows** (on a real Windows 10 and 11 PC, not CI)
- The installer runs without admin, SmartScreen text matches the notes, the app starts with no console windows flashing.
- Folder picker dialog, clipboard pre-fill, "Show in folder".
- A direct download with pause/resume, a YouTube video, an X image through the extension, a playlist.
- Chrome finds the host after install; the app starts from the extension and stays open after Chrome closes; a second app start shows the first window.
- Uninstall removes the app, the registry key and the manifest file; downloads and settings are kept or removed as the uninstaller says.
- A downloaded `.exe` shows the "downloaded from the internet" warning (Mark of the Web).

**Extension**
- Chrome loads the unzipped release zip unpacked, the ID shown is `ikplfdbdfikcokpegkapjdedlgifmkbe`, and it reaches the installed app on each OS.

**Release**
- The draft has its files (two DMGs, the extension zip, and the Windows installer once phase B is in) plus the ffmpeg source and `SHA256SUMS`; the sums match, `gh attestation verify` passes for each file, and the notes match how it was signed.
- All of the above pass on the files attached to the draft before it is published.
