# Making a release

For the owner. The workflow is `.github/workflows/release.yml`; the plan behind it is `docs/release-plan.md` and the decisions are in `docs/decisions-release.md`.

**A draft is never published until every hand check in `docs/release-plan.md` section 9 has passed on the files attached to that draft** (downloaded from the draft, not a local build). A failed check means a fix and a new tag. Never replace or edit a draft's files by hand.

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
gh secret set APPLE_CERTIFICATE -R kumarsumit17/ks-download-manager < cert.txt
gh secret set APPLE_CERTIFICATE_PASSWORD -R kumarsumit17/ks-download-manager   # prompts for the value
# … the same for the other four
rm cert.p12 cert.txt
```

Whether a release is signed is decided by the secrets that exist **when the tag is pushed**. For v0.1.0: ad hoc unless the six secrets are set by then.

Windows (phase B) is unsigned in its first release; the intended route afterwards is SignPath Foundation (draft policy: `docs/code-signing-policy.md`). Not set up yet.

## 2. Branches and the tag

Work happens on a branch (`release-setup` for the release work), is merged into `dev`, then `dev` into `main`. **The tag is made on `main`.** The workflow fails at its first step if the tagged commit is not on `main`, if the tag is not `vMAJOR.MINOR.PATCH`, or if `CHANGELOG.md` has no dated section for that version.

Before tagging, CI (`.github/workflows/ci.yml`) must be green on `main`.

```bash
# 0. Date the changelog section on the work branch: "## [0.1.0] - unreleased" -> "## [0.1.0] - YYYY-MM-DD"
git switch release-setup
#    edit CHANGELOG.md, then:
git commit -am "docs: changelog for v0.1.0"
git push origin release-setup              # wait for CI to pass on the branch

# 1. release-setup into dev
git switch dev
git pull --ff-only origin dev
git merge --no-ff release-setup -m "Merge branch 'release-setup' into dev"
git push origin dev                        # wait for CI to pass on dev

# 2. dev into main
git switch main
git pull --ff-only origin main
git merge --no-ff dev -m "Merge branch 'dev' into main"
git push origin main                       # wait for CI to pass on main

# 3. Tag main and push only the tag
git tag -a v0.1.0 -m "v0.1.0"
git push origin v0.1.0
```

Check before step 3 that you are on `main` and it matches `origin/main` (`git status -sb` shows `## main...origin/main` with nothing ahead or behind).

## 3. What the run does

Actions tab → **Release** → the run for the tag:

1. `version`: checks the tag, that it is on `main`, and the changelog section.
2. `mac (aarch64)` and `mac (x86_64)`: build the app, check the licence files are inside, run the bundled programs, sign (and with the secrets, notarise and staple), make the disk image after signing.
3. `extension`: builds the zip and checks the extension ID `ikplfdbdfikcokpegkapjdedlgifmkbe` and the version in the zipped manifest.
4. `attest`: adds the ffmpeg source tarball (hash-checked against `src-tauri/sidecars.json`), writes `SHA256SUMS`, and records a build-provenance attestation for every file.
5. `release`: checks the sums and each attestation again, writes the notes from `CHANGELOG.md` plus how the files were signed, and creates the **draft**.

## 4. Checking the draft

Releases page → the draft → download every file into an empty folder, then:

```bash
shasum -a 256 -c SHA256SUMS
for f in *.dmg *.zip *.tar.xz SHA256SUMS; do gh attestation verify "$f" -R kumarsumit17/ks-download-manager; done
```

Read the notes: they must say "signed with a Developer ID and notarised" only if the secrets were set, and otherwise give the "Open Anyway" steps. Then do every hand check in `docs/release-plan.md` section 9 on these files.

## 5. Publishing

Only when every check in section 4 has passed: open the draft, choose **Edit**, then **Publish release**. Or: `gh release edit v0.1.0 --draft=false -R kumarsumit17/ks-download-manager`.

## 6. When something fails

- **A build job fails** (no draft yet): read the log (`gh run view <run id> --log-failed`), fix on a branch, merge through `dev` into `main` as in section 2, and tag a **new** version (`v0.1.1`). Or, if nothing was published and the tag only failed in CI because of the runner, re-run the failed jobs from the Actions page: the same tag builds the same commit.
- **A run is re-run on the same tag:** the `release` job replaces an existing **draft** for that tag. It never touches a published release; it fails instead.
- **A hand check fails on the draft:** delete the draft (`gh release delete v0.1.0 -R kumarsumit17/ks-download-manager --yes`, which keeps the tag), fix, and release a new version. Tags that were pushed are never moved or deleted.
