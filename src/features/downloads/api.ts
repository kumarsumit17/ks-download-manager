import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isMsg, msgText, t } from "@/lib/i18n";
import type { Active, Category, DownloadRecord, ProbeView, Queue, Tick } from "./types";

export type ComponentUpdate =
  | { result: "upToDate"; version: string }
  | { result: "updated"; from: string; to: string };

/** Rust's `settings::Settings`. Speed limit in bytes per second. */
export interface Settings {
  maxActive: number;
  totalConnections: number;
  perDownload: number;
  speedLimit: number | null;
  folders: Record<Category, string | null>;
  preferMp4: boolean;
  /** "system", "en" or "hi". */
  language: string;
  /** The first-run notice was accepted. */
  noticeSeen: boolean;
}

/** A leftover download file or temp folder found by the last scan. */
export interface LeftoverItem {
  path: string;
  /** A video temp folder, a `.ksdownload` file, or an empty reservation file. */
  what: "folder" | "part" | "reservation";
  size: number;
}

// Every command that acts on a download takes its id; Rust looks up the path.
export const api = {
  list: () => invoke<DownloadRecord[]>("list_downloads"),
  defaultFolder: () => invoke<string>("default_folder"),
  /** The clipboard's text, only when it is exactly one web link (checked in Rust). */
  clipboardLink: () => invoke<string | null>("clipboard_link"),
  acceptNotice: () => invoke<void>("accept_notice"),
  probe: (url: string) => invoke<ProbeView>("probe_url", { url }),
  add: (url: string, name: string, folder: string, start: boolean) =>
    invoke<DownloadRecord>("add_download", { url, name, folder, start }),
  /** Adds the ticked entries (position and link, both checked in Rust); returns how many. */
  addPlaylist: (url: string, folder: string, picks: { index: number; url: string }[], number: boolean, start: boolean) =>
    invoke<number>("add_playlist", { url, folder, picks, number, start }),
  resumePlaylist: (playlist: number) => invoke<number>("resume_playlist", { playlist }),
  pausePlaylist: (playlist: number) => invoke<number>("pause_playlist", { playlist }),
  /** A playlist entry's thumbnail as image bytes, fetched by Rust. */
  thumbnail: (url: string, index: number) => invoke<ArrayBuffer>("playlist_thumbnail", { url, index }),
  resume: (id: number) => invoke<void>("resume_download", { id }),
  pause: (id: number) => invoke<void>("pause_download", { id }),
  remove: (id: number, deleteFile: boolean) => invoke<void>("delete_download", { id, deleteFile }),
  clearAll: () => invoke<void>("clear_all"),
  openFile: (id: number) => invoke<void>("open_file", { id }),
  openFolder: (id: number) => invoke<void>("open_folder", { id }),
  active: () => invoke<Active>("active_downloads"),
  updateVideoComponent: () => invoke<ComponentUpdate>("update_video_component"),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  categoryFolder: (category: Category) => invoke<string | null>("category_folder", { category }),
  pickFolder: (prompt: string, start: string | null) => invoke<string | null>("pick_folder", { prompt, start }),
  queues: () => invoke<Queue[]>("list_queues"),
  createQueue: (name: string) => invoke<Queue>("create_queue", { name }),
  renameQueue: (id: number, name: string) => invoke<void>("rename_queue", { id, name }),
  deleteQueue: (id: number) => invoke<void>("delete_queue", { id }),
  moveToQueue: (id: number, queue: number | null) => invoke<void>("move_to_queue", { id, queue }),
  startQueue: (queue: number | null) => invoke<void>("start_queue", { queue }),
  stopQueue: (queue: number | null) => invoke<number>("stop_queue", { queue }),
  leftovers: (rescan: boolean) => invoke<LeftoverItem[]>("leftover_files", { rescan }),
  /** Removes the ticked items of the last scan; each is re-checked first. */
  removeLeftovers: (paths: string[]) => invoke<[number, number]>("remove_leftover_files", { paths }),
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

export function onQueues(f: (q: Queue[]) => void): Promise<UnlistenFn> {
  return listen<Queue[]>("queues:changed", (e) => f(e.payload));
}

export function onQuitRequested(f: (a: Active) => void): Promise<UnlistenFn> {
  return listen<Active>("app:quit-requested", (e) => f(e.payload));
}

/** Rust commands reject with a message code and parameters, shown in the interface
 * language. Anything else (a browser error) falls back to the shared "went wrong" text. */
export function message(e: unknown): string {
  if (isMsg(e)) return msgText(e);
  return t("common.error.generic");
}
