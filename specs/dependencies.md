---
type: Policy
title: Dependencies
description: Third-party dependencies are kept to the minimum; the order of preference and what must be reported for each new one.
tags: [dependencies, policy]
---

# Dependencies

Third-party dependencies are kept to the minimum.

- Add a dependency only when writing it ourselves would be unreasonable or unsafe. Order of preference: the standard library or a small amount of our own code; then a crate or package already in `Cargo.lock` / `package-lock.json` (Tauri brings many); a new one last.
- Always use a maintained library for TLS, cryptography and other security-critical code. Never hand-write those.
- Enable only the features used. No dependency for convenience alone (error-type macros, argument parsers, test helpers, small utilities): write those by hand.
- Libraries named in the other specs are candidates, not commitments. Justify each at the stage that first needs it.
- Every new dependency is reported with the reason it is needed and how many packages it adds to the lockfile, and recorded in `docs/THIRD_PARTY.md` in the same commit.
