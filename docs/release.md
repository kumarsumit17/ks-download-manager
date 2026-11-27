# Making a release

For the owner. The workflow is `.github/workflows/release.yml`; the plan behind it is `docs/release-plan.md` and the decisions are in `docs/decisions-release.md`.

**The cycle** (the same for every KS-Suite app: [Release cycle](https://github.com/kumasumit/ks-suite/blob/main/specs/release-cycle.md)): work and test on `dev` (CI green), make a DEV build (a rehearsal on `dev`, section 4) for the owner to test by hand, open a release pull request from `dev` to `main` that records the owner's approval, and only with that approval fast-forward `main` to `dev` and push the tag; the workflow then publishes the release once every build and check has passed. There is no draft step. A published release's files are never changed, and a pushed tag is never moved or deleted.

**DEV, test, approval, production** (suite rule since 2026-11-27, [Release cycle](https://github.com/kumasumit/ks-suite/blob/main/specs/release-cycle.md)): every feature or fix is made on `dev`, which starts from `main` (the latest production release), and tested automatically (unit tests, CI on macOS and Windows, and the app on this Mac where it can be run). Its DEV build is the release workflow's rehearsal on `dev`: the files stay as the run's artifacts, never a release, for the owner to test by hand. **A production tag** (`vX.Y.Z`, `mac-v…`, `windows-v…` or `chrome-v…`) **needs the owner's explicit approval for that platform**; the only exception is a platform's first-ever release (for example the first `chrome-v` tag). A new ks-utility version moves the pin on `dev` but never starts a release by itself. macOS and Windows may ship unsigned. Every release's notes name the ks-utility version it was built with. Never commit or release while a test fails. A feature raises MINOR, a fix raises PATCH. v0.4.1 (ks-utility v0.18.4) was the initial rollout under this rule.

## 1. Secrets (optional: Developer ID signing on macOS)

Without any Apple secret, the macOS disk images are signed ad hoc and the release notes give the "Open Anyway" steps. With all six, they are signed with your Developer ID and notarised. Setting only some of them fails the build on purpose.

| Secret | What it is | Where it comes from |
|---|---|---|
| `APPLE_CERTIFICATE` | The "Developer ID Application" certificate with its private key, as a `.p12`, base64-encoded | Keychain Access → My Certificates → the certificate → Export (.p12), then `base64 -i cert.p12 -o cert.txt` |
| `APPLE_CERTIFICATE_PASSWORD` | The password you gave the `.p12` when exporting | You choose it |
| `APPLE_SIGNING_IDENTITY` | The certificate's full name, e.g. `Developer ID Application: Name (TEAMID)` | `security find-identity -v -p codesigning` |
| `APPLE_ID` | The Apple ID e-mail of the developer account | Your account |
| `APPLE_PASSWORD` | An **app-specific password** for that Apple ID (not the account password) | account.apple.com → Sign-In and Security → App-Specific Passwords |
| `APPLE_TEAM_ID` | The 10-character team ID | developer.apple.com → Account → Membership details |

Add each from a file so it never appears in your shell history, then delete the files:

```bash
gh secret set APPLE_CERTIFICATE -R kumasumit/ks-download-manager < cert.txt
gh secret set APPLE_CERTIFICATE_PASSWORD -R kumasumit/ks-download-manager   # prompts for the value
# … the same for the other four
rm cert.p12 cert.txt
```

Whether a release is signed is decided by the secrets that exist **when the tag is pushed**. For v0.1.2, the first release: ad hoc unless the six secrets are set by then (none were).

The Windows installer (beta, phase B; built by the `windows` job, switched on in the `version` job's outputs) is unsigned in its first release; the intended route afterwards is SignPath Foundation (draft policy: `docs/code-signing-policy.md`). Not set up yet.

## 2. The cycle: dev, main, tag

Work happens on `dev`, then `main` fast-forwards to `dev`. **The tag is made on `main`.** Tags: `vX.Y.Z` releases every platform (macOS, Windows and the Chrome extension); `mac-vX.Y.Z`, `windows-vX.Y.Z` or `chrome-vX.Y.Z` releases that one platform, so each platform has its own version. The workflow fails at its first step if the tagged commit is not on `main`, if the tag is not one of these, if it is not higher than every earlier plain tag and every earlier tag of the same platform, or if `CHANGELOG.md` has no dated section for it: `## [X.Y.Z] - YYYY-MM-DD` for a plain tag, `## [macOS X.Y.Z] - …`, `## [Windows X.Y.Z] - …` or `## [Chrome X.Y.Z] - …` for a platform tag. A plain release is marked latest; a platform release is not (the app's update check reads the release list and takes its own platform's newest). Never tag while anything is red.

The checks before the build and the attesting and publishing after it are ks-utility's shared workflows (`tauri-app-release-check.yml` and `tauri-app-release-publish.yml`, called at `@main` in `release.yml`, which is always ks-utility's newest release; a rehearsal may call `@dev` to try a change to them before its tag); the build jobs are this app's own.

```bash
# 1. On dev: the changelog section for the version is dated ("## [X.Y.Z] - YYYY-MM-DD"),
#    CI is green, and a rehearsal of that version is green (section 4)
git switch dev
git pull --ff-only origin dev
git push origin dev                        # wait for CI to pass on dev
gh workflow run release.yml --ref dev -f version=X.Y.Z -f platform=all -R kumasumit/ks-download-manager
                                           # (platform: all, mac, windows or chrome, as the tag)
                                           # wait for the rehearsal to pass; check its file list

# 2. dev into main
git switch main
git pull --ff-only origin main
git merge --ff-only dev
git push origin main                       # wait for CI to pass on main

# 3. Check what will be tagged: origin/main, with the dated section; then tag it and push only the tag
git fetch origin
git merge-base --is-ancestor main origin/main && git diff --quiet main origin/main
git show main:CHANGELOG.md | grep -E '^## \[X\.Y\.Z\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$'
git tag -a vX.Y.Z -m "vX.Y.Z" main      # or mac-vX.Y.Z, windows-vX.Y.Z, chrome-vX.Y.Z
git push origin vX.Y.Z                     # the workflow builds, checks and publishes
```

Check before step 3 that you are on `main` and it matches `origin/main` (`git status -sb` shows `## main...origin/main` with nothing ahead or behind).

## 3. What the run does

Actions tab → **Release** → the run for the tag:

1. `check` (shared): checks the tag, that it is on `main` and higher than the tags before it, the changelog section, and the version (`set-version.mjs`).
2. `plan`: names exactly the release files for the platform: the two disk images (mac), the Windows installer (windows), the extension zip (chrome), and the ffmpeg source tarball whenever macOS or Windows files are released.
3. `ffmpeg-source`: fetches the ffmpeg source from its pinned URL and checks its hash.
4. `mac (aarch64)` and `mac (x86_64)`: build the app, check the licence files are inside, run the bundled programs, sign (and with the secrets, notarise and staple), make the disk image after signing.
5. `ffmpeg-windows` and `windows`: cross-build ffmpeg, build the installer, install it, check it and uninstall it.
6. `extension`: builds the zip and checks the extension ID `ikplfdbdfikcokpegkapjdedlgifmkbe` and the version in the zipped manifest.
7. `publish` (shared): takes exactly the planned files by name (nothing else is ever attached), attests each one, checks they arrived unchanged and are attested, writes the notes (`release-notes.mjs`: the changelog section plus how the files were signed), and **publishes the release**. It runs only if every job before it passed; jobs a platform release does not need are skipped.

## 4. Rehearsal: the DEV build (before every tag)

**Actions → Release → Run workflow**, pick `dev` and give the version (`X.Y.Z`, with its dated changelog section on `dev`), or

```bash
gh workflow run release.yml --ref dev -f version=X.Y.Z -f platform=all -R kumasumit/ks-download-manager
```

Every job runs as for a tag, except the check that the tag is on `main`, and it builds the **DEV variant**: `src-tauri/tauri.dev.conf.json` (identifier `com.ks.downloadmanager.dev`, name "KS Download Manager DEV", the orange DEV icon from `assets/app-icon-dev.svg`, its own WiX upgrade code, the Cargo feature `dev`) and the DEV extension (`node extension/build.mjs --dev`: ID `mhgmalnfheamcgcongjgdmepagonmmbc`, " DEV" in its name). Its files are named `ks-download-manager-dev-…`. Nothing is attested and nothing is ever published; the files (`release-files`) and the notes (`release-notes`) are kept as the run's artifacts. Check the file list: exactly the planned files (five for `all`).

**DEV next to production** (suite rule "DEV and PROD side by side"): the DEV app has its own data folder (from its identifier), its own single-instance socket or pipe, its own native messaging host (`com.ks.downloadmanager.dev`, registered with Chrome and Edge next to the production one; its uninstaller removes only its own), its own install folder on Windows ("KS Download Manager DEV"), and download markers of its own, so it never continues or removes what the production app owns. It never checks for updates and offers no production file ("This is a DEV build…" in Settings). The DEV extension talks only to the DEV app, the production extension only to the production app. Production's identifier, upgrade code and extension ID never change. To build it locally: `npm run tauri build -- --bundles app --config src-tauri/tauri.dev.conf.json` and `npm run build:extension:dev`.

## 5. After the release: checks and hand checks

Releases page → the release → download every file into an empty folder, then:

```bash
ls                       # two .dmg, the -setup-beta.exe, the extension .zip, ffmpeg-<v>.tar.xz; nothing else
for f in *; do gh attestation verify "$f" -R kumasumit/ks-download-manager \
  --signer-workflow kumasumit/ks-utility/.github/workflows/tauri-app-release-publish.yml; done
```

Read the notes: they must say "signed with a Developer ID and notarised" only if the secrets were set, and otherwise give the "Open Anyway" steps. Then do the hand checks in `docs/release-plan.md` section 9 on these files. **Anything they find goes into the next version** (a fix on `dev` and the next tag); the published release is not changed.

## 6. When something fails

- **A build or check job fails:** nothing is published. Read the log (`gh run view <run id> --log-failed`), fix on a branch, merge through `dev` into `main` as in section 2, and tag a **new** version (the next PATCH). Or, if nothing was published and the tag only failed in CI because of the runner, re-run the failed jobs from the Actions page: the same tag builds the same commit.
- **A run is re-run on the same tag:** if an earlier attempt failed while uploading, the hidden draft it left is removed and the release is made again. A published release is never touched; the run fails instead.
- **A hand check fails after the release:** fix on `dev` and release the next version. The published release and its files stay as they are. Tags that were pushed are never moved or deleted. Example: v0.1.0 was pushed on a commit with an undated changelog section; the `version` job refused it, the tag stayed, and the release became v0.1.1. v0.1.1 then failed in the extension job (an environment variable named `ZIP`); its tag stayed too, and the next tag was v0.1.2 after a rehearsal.
