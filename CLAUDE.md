# ks-download-manager

Free download manager for images, audio and video, with up to 20× faster downloads. Part of KS-Suite; the suite-wide rules in the workspace `CLAUDE.md` apply.

## Stack
- Targets: Windows and macOS desktop. A Chrome extension is a possible add-on. No Android build for now (kept for the future).
- Rust for the download engine and all file and network work. Tauri for the desktop shell. TypeScript + React for the interface, with Tailwind CSS, shadcn/ui components and Lucide icons.
- Chrome extension: TypeScript, Manifest V3, in `extension/`. It hands links to the desktop app, and for browser downloads the cookies, referring page and user agent they need (used for that download only, never saved; `specs/extension.md`); downloading stays in Rust.
- The suite's Kotlin, Gradle and package-root rules do not apply here. Bundle identifier: `com.ks.downloadmanager`.
- Planned layout: `src-tauri/` (Rust), `src/` (interface), `extension/` (Chrome).
- Generated 2026-11-21 (Stage 1). Toolchain: Rust 1.100 (rustup, stable), Node 26 / npm 11. Tauri 2.12, React 19.3, TypeScript 6.0, Vite 8.3, Tailwind CSS 4.3, shadcn/ui (CLI 4.23, style radix-nova, Radix base), lucide-react 1.77. Full list with licences: `docs/THIRD_PARTY.md`.
- Toolchain is pinned: `rust-toolchain.toml` (Rust 1.100 + clippy) and `engines` in `package.json` (Node >= 26, npm >= 11).
- Theme: light/dark follows the system through a `prefers-color-scheme` media query in `src/index.css` (no JS). The main window starts hidden and Rust shows it after setting a matching background, so there is no white flash.
- TypeScript: `tsconfig.app.json` (src) and `tsconfig.node.json` (vite.config.ts), both checked by `tsc -b` in `npm run build`.
- CSP (`tauri.conf.json`): `connect-src ipc: http://ipc.localhost` for Tauri IPC. `dangerousDisableAssetCspModification: ["style-src"]` stops Tauri adding a style nonce, which would make browsers ignore `'unsafe-inline'` and block the `<style>` tag Radix dialogs inject. Scripts keep Tauri's hashes and nonces.
- Version placeholder: `src-tauri/Cargo.toml` has `version = "0.0.0"` because the Tauri CLI requires the field; `tauri.conf.json` and `package.json` have no version. Release builds pass the tag version (e.g. `tauri build --config '{"version":"1.2.3"}'`).
- Build stages: `specs/stages.md`. One stage per request; Stage 5 (Final) is built in one go, part by part, with decisions recorded in `docs/decisions-stage5.md`.

## Commands
```bash
npm install                      # first time
npm run tauri dev                # run the app with hot reload
npm run build                    # type-check (tsc -b, incl. vite.config.ts) + build the interface
node --test scripts/test/         # release scripts (version from tag, extension ID, notes, notices)
npm run test:lang                # language files (src/locales, extension/_locales): ks-utility's check
npm run hindi-review             # regenerate docs/hindi-review.md after changing a language file
cd src-tauri && cargo test       # Rust tests
cd src-tauri && cargo clippy --all-targets -- -D warnings
npm run tauri build              # release bundle (.app/.dmg on macOS)
```

## Working efficiently (keep token use low)
- Use the knowledge graph first: follow the rules in the `graphify` section at the end of this file. [graphify](https://github.com/Graphify-Labs/graphify) is a development tool only; it is not a dependency of the app and is never bundled or shipped.
- One-time setup on a new machine (owner): `uv tool install graphifyy` (the package name has two y's), `graphify install --project`, then `/graphify .` in Claude Code.
- Read only the specs the current stage needs, and do not re-read a file that has not changed in this session.
- Never read generated or vendored files: `node_modules/`, `dist/`, `src-tauri/target/`, `src-tauri/gen/`, `package-lock.json`, `Cargo.lock`, `graphify-out/graph.json`.
- Keep command output short (for example `cargo test -q`, and show only failures), and keep summaries brief.

## Specs
- All specs for this project live in `specs/` in the repository root, as an Open Knowledge Format (OKF) v0.2 bundle. Never create spec files anywhere else. Start at `specs/index.md`.
- One topic per file. Every file except `index.md` and `log.md` starts with YAML frontmatter: `type` (required: `Specification`, `Policy` or `Plan`), `title`, `description` (one sentence), `tags`.
- When a spec is added: list it in `specs/index.md` with its description. When a spec is added or changed: add a line under today's date (`# YYYY-MM-DD`, newest first) in `specs/log.md`.
- Link between specs with relative markdown links (`[UI direction](ui.md)`), so they also work on GitHub.

## UI
- Interface text is English and Hindi: every string is a key in `src/locales/{en,hi}.json` or ks-utility's `common.*` catalogue, used with `t()` from `src/lib/i18n.ts`; Rust sends message codes (`msg!`), never sentences. Hindi follows ks-utility's `specs/hindi-style.md`.
- Follow `specs/ui.md` (IDM-style single window: toolbar, sidebar, download list). Read it before any interface work.
- The design is our own: never copy icons, skins, screenshots or code from the reference apps.
- Component reference: https://ui.shadcn.com/ (docs, component list and CLI). Check it for the current API before adding or changing a shadcn/ui component, instead of guessing or reading `node_modules`.

## Clean room (non-negotiable)
- Follow `specs/clean-room.md`. Never read, copy or adapt source code, binaries or assets of other download managers. Implement only from our own specs and public standards.
- Every dependency is listed with its licence in `docs/THIRD_PARTY.md`.

## Engine and speed
- Follow `specs/engine.md`. The "up to 20×" claim is only made with measurements recorded in `docs/benchmarks.md`; never state a speed-up that was not measured.

## Video and playlists
- Follow `specs/video-playlists.md`: site support through yt-dlp as an unmodified sidecar, plus ffmpeg. No DRM bypass.

## Dependencies
- Keep third-party dependencies to the minimum. Follow `specs/dependencies.md` before adding any crate or package.

## Platform folders
- Every KS-Suite app repo has the same top-level platform folders: `tv/google-tv/`, `desktop/mac/`, `desktop/windows/`, `mobile/android/` and `mobile/ios/`. All five exist in every app repo, whether or not the app supports that platform yet.
- This app targets `desktop/mac` and `desktop/windows`. The other folders are empty on purpose (a `.gitkeep` only): they mark where that platform's code would go, not missing work. Do not add code for a platform that is not a target without the owner asking.
- Three layers, narrowest first: a platform folder holds code for one platform of this app; `shared/` in the repository root holds code used by two or more platforms of this app, written once and never copied between platform folders; `ks-utility` holds generic code used by several apps. Here `shared/` will hold the Tauri codebase both desktop targets build from (`src/`, `src-tauri/`, `extension/`) once the code is moved.
- One Tauri codebase builds both desktop targets, and it is still in the repository root (`src/`, `src-tauri/`, `extension/`). Moving code is a separate, planned change, done as its own stage with the builds run. Only what is specific to one operating system (packaging, signing, installer and OS-only code) goes into `desktop/mac/` or `desktop/windows/`; code both targets share goes in `shared/`.
- Never rename, merge or remove these folders, and never add another top-level platform folder; the layout is a suite-wide rule.

## Shared code (`ks-utility`)
- Generic utility code shared across the KS-Suite apps lives in the `ks-utility` repository: TypeScript in `typescript/`, Rust in `rust/`. It is first-party, has no third-party dependencies, and knows nothing about this app. Application-specific logic stays in this repo.
- It is a dependency pinned to one version tag (`vMAJOR.MINOR.PATCH`), the same in every manifest: `"ks-utility": "git+https://github.com/kumarsumit17/ks-utility.git#<tag>"` in `package.json`, and `ks-utility = { git = "https://github.com/kumarsumit17/ks-utility", tag = "<tag>" }` in `src-tauri/Cargo.toml` and `src-tauri/engine/Cargo.toml`. Both lock files record the exact commit behind the tag, so builds stay reproducible. Tags are never moved; versioning rules are in ks-utility's `specs/usage.md`.
- Before writing a generic helper, look in `ks-utility` and use it from there. If it is missing and another app would need it too, add it in the `ks-utility` repository, tag it there, then move the pin here in its own commit (change the tag, `npm install`, `cargo check`, check both lock files name the tag's commit, commit both lock files).
- The clean-room rule covers `ks-utility` too.

## Constraints
- Free: no ads, no analytics, no crash-reporting SDKs.
- Network access is used only for the downloads the user starts, with one exception: the daily app update check (on by default, a switch in Settings turns it off; `specs/ui.md`, "App updates").
- No links to other KS apps. If one is added, record the contract in the suite README (Cross-Repository Relationships).

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- When graphify-out/graph.json exists, run `graphify query "<question>"` before your first grep, search or source-file read for a codebase question, and put this rule in every subagent prompt that explores code. Read raw files only for lines the graph or an earlier read already located. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. When the answer is under a Markdown heading, `graphify section "<heading>"` prints that section's text, so the .md need not be opened. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
- `graphify-out/` is generated and git-ignored: never commit it.
