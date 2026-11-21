# Clean-room rule

KS Download Manager is an independent implementation. Other download managers are used only to understand what users expect, never as a source of code or assets.

## Allowed
- Using the reference apps as an ordinary user, and reading their public feature pages and user documentation, to learn **what** they do.
- Writing our own specs in our own words (`docs/spec/`), then implementing from those specs.
- Public standards and protocol documents (HTTP range requests, HLS, DASH, Chrome extension APIs).
- Third-party libraries and tools used as declared dependencies under their own licences, each listed in `docs/THIRD_PARTY.md` with its licence.

## Not allowed
- Reading, copying, translating or paraphrasing source code of Internet Download Manager, AB Download Manager, Free Download Manager or any other download manager. This includes "just checking how they did it".
- Decompiling, disassembling or inspecting the binaries, network traffic or extension code of those apps.
- Copying their icons, skins, screenshots, sounds, layouts pixel-for-pixel, names, or wording from their UI, sites or docs.
- Pasting code of unknown origin (forums, gists, AI output that reproduces a known project) without checking it is original or properly licensed.

## Working practice
- Every feature starts from a spec in `docs/spec/` that describes behaviour, not another app's implementation.
- If someone has read a reference app's source for a feature, they do not implement that feature here.
- Commit messages and specs never cite another app's code as the basis for a change.
- New dependency: record name, version, licence and why it is needed in `docs/THIRD_PARTY.md` in the same commit.
