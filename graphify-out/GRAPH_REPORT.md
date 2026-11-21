# Graph Report - ks-download-manager  (2026-11-21)

## Corpus Check
- Corpus is ~23,048 words - fits in a single context window. You may not need a graph.

## Summary
- 605 nodes · 1225 edges · 36 communities (25 shown, 11 thin omitted)
- Extraction: 92% EXTRACTED · 8% INFERRED · 0% AMBIGUOUS · INFERRED: 99 edges (avg confidence: 0.85)
- Token cost: 105,070 input · 0 output

## Community Hubs (Navigation)
- Segmented Download Core
- Frontend UI Components
- Engine Example and Tests
- npm Package Manifest
- HTTP Probe and Headers
- Test Server Fixtures
- Range Claims and Stealing
- shadcn Components Config
- Retry and Backoff
- Range Map Persistence
- App TS Config
- Project Rules and Clean Room
- Cancel and Progress Control
- Tauri App Config
- Node TS Config
- Engine Spec Concepts
- Stack and Stage 1
- Roadmap and Release Stages
- UI and Video Spec
- Resume and Splitting Spec
- Tauri Shell and Theme
- Tauri Capabilities
- Root TS Config
- App Icon Design
- 20x Speed Claim
- Web Entry and Vite
- Pinned Toolchain
- Own Icon Design
- Tauri JS API
- Engine Crate
- Project Root
- Platform Roadmap

## God Nodes (most connected - your core abstractions)
1. `EngineError` - 38 edges
2. `download()` - 31 edges
3. `opts()` - 24 edges
4. `Download engine spec` - 19 edges
5. `segmented()` - 18 edges
6. `Table` - 18 edges
7. `fetch_ok()` - 18 edges
8. `compilerOptions` - 18 edges
9. `probe()` - 17 edges
10. `TestServer` - 17 edges

## Surprising Connections (you probably didn't know these)
- `Stack (Rust + Tauri + React/TS, Tailwind, shadcn/ui, Lucide)` --semantically_similar_to--> `UI stack (React/TS/Vite, Tailwind tokens, shadcn/ui, TanStack, Zustand, Lucide)`  [INFERRED] [semantically similar]
  CLAUDE.md → specs/ui.md
- `tokio 1.54.1` --implements--> `Download engine spec`  [INFERRED]
  docs/THIRD_PARTY.md → specs/engine.md
- `rustls + aws-lc-rs + rustls-platform-verifier TLS chain` --conceptually_related_to--> `Dependencies policy (minimum third-party deps)`  [INFERRED]
  docs/THIRD_PARTY.md → specs/dependencies.md
- `UI stack (React/TS/Vite, Tailwind tokens, shadcn/ui, TanStack, Zustand, Lucide)` --references--> `tailwindcss 4.3.5`  [INFERRED]
  specs/ui.md → docs/THIRD_PARTY.md
- `graphify skill trigger (/graphify)` --conceptually_related_to--> `graphify knowledge-graph rules`  [INFERRED]
  .claude/CLAUDE.md → CLAUDE.md

## Import Cycles
- 1-file cycle: `src-tauri/engine/src/http.rs -> src-tauri/engine/src/http.rs`
- 2-file cycle: `src-tauri/engine/src/download.rs -> src-tauri/engine/src/ranges.rs -> src-tauri/engine/src/download.rs`
- 3-file cycle: `src-tauri/engine/src/download.rs -> src-tauri/engine/src/probe.rs -> src-tauri/engine/src/ranges.rs -> src-tauri/engine/src/download.rs`
- 3-file cycle: `src-tauri/engine/src/download.rs -> src-tauri/engine/src/rangemap.rs -> src-tauri/engine/src/ranges.rs -> src-tauri/engine/src/download.rs`
- 4-file cycle: `src-tauri/engine/src/download.rs -> src-tauri/engine/src/rangemap.rs -> src-tauri/engine/src/probe.rs -> src-tauri/engine/src/ranges.rs -> src-tauri/engine/src/download.rs`

## Hyperedges (group relationships)
- **Crash-safe resume flow** — specs_engine_probe, specs_engine_strong_validator, specs_engine_ksdm_range_map, specs_engine_resume_rules, specs_engine_crash_safe_resume [EXTRACTED 1.00]
- **20x speed claim proof chain** — readme_20x_goal, specs_engine_proving_the_claim, specs_engine_benchmarks_md, specs_stages_stage6_benchmarks [INFERRED 0.85]
- **Dependency and licence governance** — specs_dependencies_dependencies_policy, specs_clean_room_working_practice, docs_third_party_third_party_components, specs_stages_done_means [INFERRED 0.85]

## Communities (36 total, 11 thin omitted)

### Community 0 - "Segmented Download Core"
Cohesion: 0.05
Nodes (43): CancelToken, check_size(), covers(), Ctx, discard(), DownloadOptions, DownloadOutcome, Failure (+35 more)

### Community 1 - "Frontend UI Components"
Cohesion: 0.07
Nodes (56): dependencies, class-variance-authority@^0.7.1, cn@^0.5.2, lucide-react@^1.77.0, radix-ui@^1.8.1, react@^19.1.0, react-dom@^19.1.0, class-variance-authority (+48 more)

### Community 2 - "Engine Example and Tests"
Cohesion: 0.12
Nodes (45): main(), mb(), DEFAULT_CONNECTIONS, download(), saved_state(), a_damaged_range_map_is_ignored(), a_download_without_a_validator_is_never_reported_resumable(), a_fresh_start_removes_the_old_map_before_touching_the_file() (+37 more)

### Community 3 - "npm Package Manifest"
Cohesion: 0.07
Nodes (36): @tauri-apps/api@^2, tw-animate-css@^1.4.0, devDependencies, shadcn@^4.23.0, tailwindcss@^4.3.5, @tailwindcss/vite@^4.3.5, @tauri-apps/cli@^2, @types/node@^26.6.6 (+28 more)

### Community 4 - "HTTP Probe and Headers"
Cohesion: 0.09
Nodes (26): client(), content_range_forms(), ContentRange, Bytes, Unsatisfied, get(), parse_content_range(), parse_url() (+18 more)

### Community 5 - "Test Server Fixtures"
Cohesion: 0.12
Nodes (18): data(), data_version(), FileState, http_date(), http_date_formats_the_rfc_example(), parse_range(), read_head(), Request (+10 more)

### Community 6 - "Range Claims and Stealing"
Cohesion: 0.14
Nodes (16): a_retired_worker_hands_its_range_to_the_others(), a_worker_that_exited_no_longer_counts_as_active(), Claim, claims_never_pass_the_end(), gaps(), gaps_are_the_complement(), idle_worker_steals_half_of_the_slowest_and_ranges_never_overlap(), MB (+8 more)

### Community 7 - "shadcn Components Config"
Cohesion: 0.09
Nodes (21): aliases, components, hooks, lib, ui, utils, iconLibrary, menuAccent (+13 more)

### Community 8 - "Retry and Backoff"
Cohesion: 0.14
Nodes (11): backoff_grows_stays_under_the_cap_and_respects_retry_after(), days_from_civil(), http_dates_in_all_three_forms(), parse_http_date(), parse_retry_after(), random_below(), retry_after(), retry_after_seconds_and_dates() (+3 more)

### Community 9 - "Range Map Persistence"
Cohesion: 0.18
Nodes (12): damaged_maps_are_rejected(), HEADER, line_breaks_cannot_be_encoded(), load(), map_path_appends_a_suffix(), path(), RangeMap, remove() (+4 more)

### Community 10 - "App TS Config"
Cohesion: 0.10
Nodes (19): compilerOptions, allowImportingTsExtensions, isolatedModules, jsx, lib, module, moduleResolution, noEmit (+11 more)

### Community 11 - "Project Rules and Clean Room"
Cohesion: 0.15
Nodes (19): graphify skill trigger (/graphify), Clean room (non-negotiable), graphify knowledge-graph rules, ks-download-manager project instructions, Working efficiently (keep token use low), tauri 2.12.7, Third-party components register, tokio 1.54.1 (+11 more)

### Community 12 - "Cancel and Progress Control"
Cohesion: 0.16
Nodes (5): CancelInner, Progress, ProgressInner, ProgressSnapshot, UNKNOWN

### Community 13 - "Tauri App Config"
Cohesion: 0.11
Nodes (17): app, security, windows, build, beforeBuildCommand, beforeDevCommand, devUrl, frontendDist (+9 more)

### Community 14 - "Node TS Config"
Cohesion: 0.12
Nodes (16): compilerOptions, allowImportingTsExtensions, isolatedModules, lib, module, moduleResolution, noEmit, noFallthroughCasesInSwitch (+8 more)

### Community 15 - "Engine Spec Concepts"
Cohesion: 0.22
Nodes (11): reqwest 0.13.5 (rustls only, HTTP/1.1), rustls + aws-lc-rs + rustls-platform-verifier TLS chain, Adaptive connection count, Direct-to-place writes (pre-allocated, ~1 MB buffers), Download engine spec, Giving up: retiring connection hands range back, Global scheduler (connection/download caps, speed limit), Being a good client (<=32 connections, honour Retry-After) (+3 more)

### Community 16 - "Stack and Stage 1"
Cohesion: 0.18
Nodes (11): Bundle identifier com.ks.downloadmanager, CSP configuration (tauri.conf.json), Stack (Rust + Tauri + React/TS, Tailwind, shadcn/ui, Lucide), lucide-react 1.77.0, radix-ui 1.8.1, shadcn/ui components (button, progress, separator, hand-written alert-dialog), tailwindcss 4.3.5, Stage 1: Scaffold and main window with sample data (+3 more)

### Community 17 - "Roadmap and Release Stages"
Cohesion: 0.18
Nodes (11): Constraints: free, no ads/analytics/crash SDKs, network only for user downloads, KS Download Manager README, KS-Suite, Release via semantic version tags, Build stages plan, Rust crate engine inside src-tauri, Stage 2: Engine core (single file), Stage 8: Polish (+3 more)

### Community 18 - "UI and Video Spec"
Cohesion: 0.23
Nodes (12): Stage 4: UI wired to the engine, Stage 7: Video and playlists, Add download dialog, Batched progress updates, Download progress panel (one bar per connection), Playlist picker dialog, Settings dialog, UI direction spec (+4 more)

### Community 19 - "Resume and Splitting Spec"
Cohesion: 0.22
Nodes (9): Crash-safe resume (If-Range revalidation), Dynamic splitting / work stealing, Range map file <file>.ksdm, Pause and failure handling, Probe first (size, range support, validator), Resume rules (URL, size, strong validator match), Strong validator (ETag / Last-Modified), Victim choice (longest estimated time left, no split under 128 KB) (+1 more)

### Community 21 - "Tauri Capabilities"
Cohesion: 0.33
Nodes (5): description, identifier, permissions, $schema, windows

### Community 22 - "Root TS Config"
Cohesion: 0.40
Nodes (4): compilerOptions, paths, files, references

### Community 23 - "App Icon Design"
Cohesion: 0.67
Nodes (4): App Icon (app-icon.svg), Download arrow glyph, Rounded-square blue gradient tile, Tray / baseline bar

### Community 24 - "20x Speed Claim"
Cohesion: 0.83
Nodes (4): Up to 20x faster downloads goal (measured before claimed), docs/benchmarks.md, Proving the 20x claim (throttled test server, benchmarks.md, CI regression), Stage 6: Benchmarks

### Community 25 - "Web Entry and Vite"
Cohesion: 0.67
Nodes (3): react / react-dom 19.3.1, vite 8.3.8, index.html root mount (loads /src/main.tsx)

## Knowledge Gaps
- **44 isolated node(s):** `ks-download-manager`, `engine`, `Retry`, `InvalidUrl`, `InvalidOptions` (+39 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 195 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **11 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `EngineError` connect `Segmented Download Core` to `Retry and Backoff`, `Engine Example and Tests`, `HTTP Probe and Headers`?**
  _High betweenness centrality (0.043) - this node is a cross-community bridge._
- **Are the 22 inferred relationships involving `download()` (e.g. with `main()` and `a_damaged_range_map_is_ignored()`) actually correct?**
  _`download()` has 22 INFERRED edges - model-reasoned connections that need verification._
- **What connects `ks-download-manager`, `engine`, `Retry` to the rest of the system?**
  _44 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Segmented Download Core` be split into smaller, more focused modules?**
  _Cohesion score 0.0531883632089333 - nodes in this community are weakly interconnected._
- **Why does `Progress` connect `Cancel and Progress Control` to `Segmented Download Core`, `Engine Example and Tests`?**
  _High betweenness centrality (0.016) - this node is a cross-community bridge._
- **Are the 2 inferred relationships involving `segmented()` (e.g. with `parse_url()` and `gaps()`) actually correct?**
  _`segmented()` has 2 INFERRED edges - model-reasoned connections that need verification._
- **Should `Frontend UI Components` be split into smaller, more focused modules?**
  _Cohesion score 0.0661189358372457 - nodes in this community are weakly interconnected._