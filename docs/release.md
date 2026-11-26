# Making a release

For the owner. The workflow is `.github/workflows/release.yml`; the plan behind it is `docs/release-plan.md` and the decisions are in `docs/decisions-release.md`.

**The cycle** (the same for every KS-Suite app: [Release cycle](https://github.com/kumasumit/KS-Suite/blob/main/specs/release-cycle.md)): test on `dev` (CI and a rehearsal green), fast-forward `main` to `dev`, push the tag, and the workflow publishes the release, a full release marked latest, once every build and check has passed. There is no draft step. Hand checks are done on the published files afterwards; anything they find goes into the next version. A published release's files are never changed, and a pushed tag is never moved or deleted.

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

Work happens on a branch, is merged into `dev`, then `dev` into `main` (fast-forward only). **The tag is made on `main`.** The workflow fails at its first step if the tagged commit is not on `main`, if the tag is not `vMAJOR.MINOR.PATCH`, or if `CHANGELOG.md` has no dated section for that version. Never tag while anything is red.

```bash
# 1. On dev: the changelog section for the version is dated ("## [X.Y.Z] - YYYY-MM-DD"),
#    CI is green, and a rehearsal of that version is green (section 4)
git switch dev
git pull --ff-only origin dev
git push origin dev                        # wait for CI to pass on dev
gh workflow run release.yml --ref dev -f version=X.Y.Z -R kumasumit/ks-download-manager
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
git tag -a vX.Y.Z -m "vX.Y.Z" main
git push origin vX.Y.Z                     # the workflow builds, checks and publishes
```

Check before step 3 that you are on `main` and it matches `origin/main` (`git status -sb` shows `## main...origin/main` with nothing ahead or behind).

## 3. What the run does

Actions tab → **Release** → the run for the tag:

1. `version`: checks the tag, that it is on `main`, and the changelog section.
2. `mac (aarch64)` and `mac (x86_64)`: build the app, check the licence files are inside, run the bundled programs, sign (and with the secrets, notarise and staple), make the disk image after signing.
3. `extension`: builds the zip and checks the extension ID `ikplfdbdfikcokpegkapjdedlgifmkbe` and the version in the zipped manifest.
4. `attest`: takes exactly the five release files by name (two disk images, the Windows installer, the extension zip, and the ffmpeg source tarball, fetched from its pinned URL and hash-checked; nothing else is ever attached) and records a build-provenance attestation for each. A `SHA256SUMS` is written for the next job only, not attached.
5. `release`: the only job with write access. Checks the files arrived unchanged, that there are exactly those files, and each attestation again; writes the notes from `CHANGELOG.md` plus how the files were signed; and **publishes the release** as a full release marked latest. It runs only if every job before it passed.

## 4. Rehearsal (before every tag)

**Actions → Release → Run workflow**, pick `dev` and give the version (`X.Y.Z`, with its dated changelog section on `dev`), or

```bash
gh workflow run release.yml --ref dev -f version=X.Y.Z -R kumasumit/ks-download-manager
```

Every job runs as for a tag, except the check that the tag is on `main`. Nothing is attested and nothing is ever published; the files (`release-files`) and the notes (`release-notes`) are kept as the run's artifacts. Check the file list: exactly the five release files.

## 5. After the release: checks and hand checks

Releases page → the release → download every file into an empty folder, then:

```bash
ls                       # two .dmg, the -setup-beta.exe, the extension .zip, ffmpeg-<v>.tar.xz; nothing else
for f in *; do gh attestation verify "$f" -R kumasumit/ks-download-manager; done
```

Read the notes: they must say "signed with a Developer ID and notarised" only if the secrets were set, and otherwise give the "Open Anyway" steps. Then do the hand checks in `docs/release-plan.md` section 9 on these files. **Anything they find goes into the next version** (a fix on `dev` and the next tag); the published release is not changed.

## 6. When something fails

- **A build or check job fails:** nothing is published. Read the log (`gh run view <run id> --log-failed`), fix on a branch, merge through `dev` into `main` as in section 2, and tag a **new** version (the next PATCH). Or, if nothing was published and the tag only failed in CI because of the runner, re-run the failed jobs from the Actions page: the same tag builds the same commit.
- **A run is re-run on the same tag:** if an earlier attempt failed while uploading, the hidden draft it left is removed and the release is made again. A published release is never touched; the run fails instead.
- **A hand check fails after the release:** fix on `dev` and release the next version. The published release and its files stay as they are. Tags that were pushed are never moved or deleted. Example: v0.1.0 was pushed on a commit with an undated changelog section; the `version` job refused it, the tag stayed, and the release became v0.1.1. v0.1.1 then failed in the extension job (an environment variable named `ZIP`); its tag stayed too, and the next tag was v0.1.2 after a rehearsal.
