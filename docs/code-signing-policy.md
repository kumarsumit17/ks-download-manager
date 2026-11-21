# Code signing policy (draft)

**Status: draft, not in effect.** Windows releases are not code-signed yet. This page is written for an application to [SignPath Foundation](https://signpath.org) (free code signing for open-source projects) and is not linked from the README until signing is set up. The owner decides whether and when to apply (`docs/release-plan.md`, section 3.2).

> Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org). *(Not yet true: this line takes effect only once signing is set up.)*

## What is signed

- Only the Windows installer of KS Download Manager, built by this repository's GitHub Actions (`.github/workflows/release.yml`) from a `vX.Y.Z` tag on `main`.
- Nothing built on a personal computer is ever signed, and no file from outside this repository's build (for example a third-party program by itself) is signed on its own.
- The installer contains unmodified third-party programs (yt-dlp, Deno) and an LGPL ffmpeg built from pinned source by the same workflow; each is listed with its licence in `docs/THIRD_PARTY.md`.

## Team and roles

| Role | Who |
|---|---|
| Committers (may push to the repository) | Kumar Sumit (owner) |
| Reviewers (review changes before they reach `main`) | Kumar Sumit (owner) |
| Approvers (approve each signing request) | Kumar Sumit (owner) |

Every release is approved by hand for signing after its draft has passed the checks in `docs/release.md`. Multi-factor authentication is required for the GitHub and SignPath accounts of everyone listed.

## Privacy

KS Download Manager does not collect or send any data about its users. It connects to the network only for the downloads the user starts (and, when the user chooses **Update video component**, to fetch yt-dlp's latest release from GitHub). It has no analytics, crash reporting or ads. The Chrome extension sends the link the user chose to the desktop app on the same computer, never cookies or page content.

## Reporting a problem

Open an issue in this repository. A signed file found to be malicious or signed by mistake is reported to SignPath, and the release is withdrawn.
