# Testing builds

How to build and test each target from a Mac. The suite-wide matrix is in the KS-Suite workspace's [specs/testing.md](https://github.com/kumarsumit17/KS-Suite/blob/main/specs/testing.md).

## macOS (on the Mac)

```bash
npm install
npm run tauri dev                                   # run with hot reload
npm run build && npm run test:lang && npm run test:ui
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
npm run tauri build                                 # .app/.dmg
```

## Windows (CI)

An Apple-silicon Mac cannot build or run the Windows installer, so Windows is tested on GitHub Actions (`windows-2025`):

- **Every push and pull request**, and a manual run (Actions > CI > Run workflow): `check-windows` runs every check on Windows; `installer-windows` builds the NSIS installer, installs it silently, checks it (`scripts/test-windows-installer.sh`) and uninstalls it.
- **The installer to try by hand:** the run's `windows-installer-unsigned` artifact (kept 14 days). It is unsigned, so Windows SmartScreen warns before it runs.
- **Release rehearsal:** Actions > Release > Run workflow builds every release file without publishing ([release steps](release.md)).

Hands-on testing needs a Windows machine, or a Windows 11 ARM virtual machine on the Mac (UTM or Parallels, with a Windows licence from the owner). Windows 11 ARM runs the x64 installer through its built-in emulation.

## Chrome extension

```bash
npm run test:extension        # unit tests
npm run build:extension       # builds extension/dist/
npm run test:lang             # includes extension/_locales
node scripts/extension-id.mjs extension/dist/manifest.json ikplfdbdfikcokpegkapjdedlgifmkbe   # the fixed ID
```

Load it: `chrome://extensions` > **Developer mode** > **Load unpacked** > `extension/dist`, after opening the desktop app once so it registers its native-messaging host. After a rebuild, press the extension's reload button. The CI run's `extension-unpacked` artifact is the same folder, built on CI.

Chrome 137 and later ignore `--load-extension` in Google Chrome itself, so loading the extension from a script needs Chrome for Testing or Chromium; loading it is a hand step here.
