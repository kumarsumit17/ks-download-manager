# Third-party components

Add one row per dependency in the commit that introduces it. Direct dependencies only; their own dependencies come with them under their own licences.

## Rust (`src-tauri/Cargo.toml`)

| Component | Version | Licence | Used for |
|---|---|---|---|
| tauri | 2.12.7 | Apache-2.0 OR MIT | Desktop shell and native window |
| tauri-build | 2.7.6 | Apache-2.0 OR MIT | Build step for the Tauri app |
| tauri-plugin-opener | 2.7.3 | Apache-2.0 OR MIT | Open finished files and folders |
| serde | 1.0.229 | Apache-2.0 OR MIT | Serialising data between Rust and the interface |
| serde_json | 1.0.154 | Apache-2.0 OR MIT | JSON for the same |

## Interface, runtime (`package.json` dependencies)

| Component | Version | Licence | Used for |
|---|---|---|---|
| react, react-dom | 19.3.1 | MIT | Interface |
| @tauri-apps/api | 2.12.5 | Apache-2.0 OR MIT | Calling Rust from the interface |
| @tauri-apps/plugin-opener | 2.7.3 | MIT OR Apache-2.0 | Interface side of the opener plugin |
| radix-ui | 1.8.1 | MIT | Accessible primitives under shadcn/ui |
| shadcn/ui components (copied into `src/components/ui/`) | CLI 4.23.0, style radix-nova | MIT | Buttons, table, progress, tooltip, separator, scroll area, badge |
| cn | 0.5.2 | MIT | Class-name merging used by shadcn/ui |
| class-variance-authority | 0.7.1 | Apache-2.0 | Component variants |
| lucide-react | 1.77.0 | ISC | Icons |
| tw-animate-css | 1.4.0 | MIT | Short animation utilities |

## Interface, build only (`package.json` devDependencies)

| Component | Version | Licence | Used for |
|---|---|---|---|
| @tauri-apps/cli | 2.12.6 | Apache-2.0 OR MIT | `tauri dev` / `tauri build` |
| vite | 8.3.8 | MIT | Dev server and bundler |
| @vitejs/plugin-react | 6.3.0 | MIT | React support in Vite |
| typescript | 6.0.3 | Apache-2.0 | Type checking |
| tailwindcss, @tailwindcss/vite | 4.3.5 | MIT | Styling |
| shadcn | 4.23.0 | MIT | CLI that adds shadcn/ui components |
| @types/react, @types/react-dom, @types/node | 19.3.0, 19.3.0, 26.6.6 | MIT | Type definitions |

## Assets
- App icon (`assets/app-icon.svg`, generated sizes in `src-tauri/icons/`): our own design, made for this project.
