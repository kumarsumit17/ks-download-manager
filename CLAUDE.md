# ks-download-manager

Free download manager for images, audio and video, with up to 20× faster downloads. Part of KS-Suite; the suite-wide rules in the workspace `CLAUDE.md` apply.

## Stack
- Targets: Windows and macOS desktop. A Chrome extension is a possible add-on. No Android build for now (kept for the future).
- Rust for the download engine and all file and network work. Tauri for the desktop shell. TypeScript + React for the interface, with Tailwind CSS, shadcn/ui components and Lucide icons.
- Chrome extension, if built: TypeScript, Manifest V3, in `extension/`. It only hands URLs to the desktop app; downloading stays in Rust.
- The suite's Kotlin, Gradle and package-root rules do not apply here. Bundle identifier: `com.ks.downloadmanager`.
- Planned layout: `src-tauri/` (Rust), `src/` (interface), `extension/` (Chrome).
- Generated 2026-11-21 (Stage 1). Toolchain: Rust 1.100 (rustup, stable), Node 26 / npm 11. Tauri 2.12, React 19.3, TypeScript 6.0, Vite 8.3, Tailwind CSS 4.3, shadcn/ui (CLI 4.23, style radix-nova, Radix base), lucide-react 1.77. Full list with licences: `docs/THIRD_PARTY.md`.
- Toolchain is pinned: `rust-toolchain.toml` (Rust 1.100 + clippy) and `engines` in `package.json` (Node >= 26, npm >= 11).
- Theme: light/dark follows the system through a `prefers-color-scheme` media query in `src/index.css` (no JS). The main window starts hidden and Rust shows it after setting a matching background, so there is no white flash.
- TypeScript: `tsconfig.app.json` (src) and `tsconfig.node.json` (vite.config.ts), both checked by `tsc -b` in `npm run build`.
- CSP (`tauri.conf.json`): `connect-src ipc: http://ipc.localhost` for Tauri IPC. `dangerousDisableAssetCspModification: ["style-src"]` stops Tauri adding a style nonce, which would make browsers ignore `'unsafe-inline'` and block the `<style>` tag Radix dialogs inject. Scripts keep Tauri's hashes and nonces.
- Version placeholder: `src-tauri/Cargo.toml` has `version = "0.0.0"` because the Tauri CLI requires the field; `tauri.conf.json` and `package.json` have no version. Release builds pass the tag version (e.g. `tauri build --config '{"version":"1.2.3"}'`).
- Build stages: `specs/stages.md`. One stage per request.

## Commands
```bash
npm install                      # first time
npm run tauri dev                # run the app with hot reload
npm run build                    # type-check (tsc -b, incl. vite.config.ts) + build the interface
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

## Shared code (`ks-utility`)
- Generic utility code shared across the KS-Suite apps lives in the `ks-utility` repository: TypeScript in `typescript/`, Rust in `rust/`. It is first-party, has no third-party dependencies, and knows nothing about this app. Application-specific logic stays in this repo.
- It is a dependency pinned to one commit, the same in both manifests: `"ks-utility": "github:kumarsumit17/ks-utility#<commit>"` in `package.json`, and `ks-utility = { git = "https://github.com/kumarsumit17/ks-utility", rev = "<commit>" }` in `src-tauri/Cargo.toml` (and in `src-tauri/engine/Cargo.toml` if the engine uses it).
- Pending (approved by the owner on 2026-11-21, not added yet because a stage was in progress): add both lines with commit `ced399ce3667f8643bcf55c9a20c03df051e9049`, run `npm install` and `cargo check`, commit `package-lock.json` and `Cargo.lock`, and list `ks-utility` in `docs/THIRD_PARTY.md` as first-party. Remove this bullet when done.
- Before writing a generic helper, look in `ks-utility` and use it from there. If it is missing and another app would need it too, add it in the `ks-utility` repository, then move the pin here (change the commit, `npm install`, `cargo check`, commit both lock files).
- The clean-room rule covers `ks-utility` too.

## Constraints
- Free: no ads, no analytics, no crash-reporting SDKs.
- Network access is used only for the downloads the user starts.
- No links to other KS apps. If one is added, record the contract in the suite README (Cross-Repository Relationships).

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- When graphify-out/graph.json exists, run `graphify query "<question>"` before your first grep, search or source-file read for a codebase question, and put this rule in every subagent prompt that explores code. Read raw files only for lines the graph or an earlier read already located. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. When the answer is under a Markdown heading, `graphify section "<heading>"` prints that section's text, so the .md need not be opened. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
- A committed graphify-out/graph.json is rewritten whole on every build, so a large diff to it in a pull request is expected. Do not flag it as churn or suggest skipping the graph rebuild or its hooks.
