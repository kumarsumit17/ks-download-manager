import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Active, DownloadRecord, ProbeView, Tick } from "./types";

export type ComponentUpdate =
  | { result: "upToDate"; version: string }
  | { result: "updated"; from: string; to: string };

// Every command that acts on a download takes its id; Rust looks up the path.
export const api = {
  list: () => invoke<DownloadRecord[]>("list_downloads"),
  defaultFolder: () => invoke<string>("default_folder"),
  probe: (url: string) => invoke<ProbeView>("probe_url", { url }),
  add: (url: string, name: string, folder: string, start: boolean) =>
    invoke<DownloadRecord>("add_download", { url, name, folder, start }),
  resume: (id: number) => invoke<void>("resume_download", { id }),
  pause: (id: number) => invoke<void>("pause_download", { id }),
  remove: (id: number, deleteFile: boolean) => invoke<void>("delete_download", { id, deleteFile }),
  clearAll: () => invoke<void>("clear_all"),
  openFile: (id: number) => invoke<void>("open_file", { id }),
  openFolder: (id: number) => invoke<void>("open_folder", { id }),
  active: () => invoke<Active>("active_downloads"),
  updateVideoComponent: () => invoke<ComponentUpdate>("update_video_component"),
  quit: () => invoke<void>("quit"),
};

export function onChanged(f: (r: DownloadRecord) => void): Promise<UnlistenFn> {
  return listen<DownloadRecord>("downloads:changed", (e) => f(e.payload));
}

export function onRemoved(f: (id: number) => void): Promise<UnlistenFn> {
  return listen<number>("downloads:removed", (e) => f(e.payload));
}

export function onProgress(f: (ticks: Tick[]) => void): Promise<UnlistenFn> {
  return listen<Tick[]>("downloads:progress", (e) => f(e.payload));
}

export function onQuitRequested(f: (a: Active) => void): Promise<UnlistenFn> {
  return listen<Active>("app:quit-requested", (e) => f(e.payload));
}

/** Rust commands reject with a plain message. */
export function message(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}
