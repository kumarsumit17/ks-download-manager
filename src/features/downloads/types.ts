import { t, type Msg } from "@/lib/i18n";
export type Category = "video" | "audio" | "images" | "other";
export type Kind = "file" | "video";
export type Status = "queued" | "downloading" | "waiting" | "paused" | "finished" | "failed" | "interrupted";

/** One download as Rust stores it (src-tauri/src/store.rs, `Record`). */
export interface DownloadRecord {
  id: number;
  /** "video": fetched by the video component (yt-dlp), not by our engine. */
  kind: Kind;
  url: string;
  name: string;
  /** Shown only; commands always name a download by id, never by path. */
  path: string;
  category: Category;
  status: Status;
  /** Total size in bytes; null when the server did not say. */
  size: number | null;
  received: number;
  /** The server supports resuming; otherwise the download cannot be paused. */
  resumable: boolean;
  error: Msg | null;
  /** What can be offered for the error: "videoOutdated" shows the update action. */
  errorCode: string | null;
  addedAt: number;
  /** The queue it belongs to; absent or null for the main queue. */
  queue?: number | null;
  /** The playlist it was added from: its rows are grouped under the playlist's title. */
  playlist?: PlaylistRef | null;
}

/** Rust's `store::PlaylistRef`. `index` is the position in the playlist, from 1. */
export interface PlaylistRef {
  id: number;
  title: string;
  index: number;
}

/** One entry of a looked-up playlist (Rust's `manager::EntryView`). */
export interface PlaylistEntryView {
  index: number;
  url: string;
  title: string;
  /** Seconds. */
  duration: number | null;
  /** Private, removed, region-blocked or a list inside the list: shown, never ticked. */
  unavailable: boolean;
  hasThumbnail: boolean;
}

export interface PlaylistView {
  title: string;
  entries: PlaylistEntryView[];
}

/** A queue the user made; the main queue is built in. */
export interface Queue {
  id: number;
  name: string;
}

/** A connection's current range [start, end), of which [start, position) has arrived. */
export interface ConnectionTick {
  start: number;
  position: number;
  end: number;
  speed: number;
}

/** Live numbers for one running download, sent a few times a second. */
export interface Tick {
  id: number;
  size: number | null;
  received: number;
  speed: number;
  connections: (ConnectionTick | null)[];
  /** A video whose streams are being joined. */
  merging: boolean;
}

export interface Download extends DownloadRecord {
  /** Bytes per second; 0 when not downloading. */
  speed: number;
  connections: (ConnectionTick | null)[];
  merging: boolean;
}

export interface ProbeView {
  kind: Kind;
  name: string;
  size: number | null;
  resumable: boolean;
  category: Category;
  /** Set when the link is a playlist. */
  playlist?: PlaylistView;
}

/** A finished video saved as MKV: most players handle it, QuickTime does not. */
export function isMkvVideo(d: DownloadRecord): boolean {
  return d.kind === "video" && d.status === "finished" && /\.mkv$/i.test(d.name);
}

export const VIDEO_OUTDATED = "videoOutdated";

export interface Active {
  running: number;
  notResumable: number;
}

export type Filter =
  | { kind: "all" }
  | { kind: "unfinished" }
  | { kind: "finished" }
  | { kind: "category"; category: Category }
  /** `queue` null: the main queue. */
  | { kind: "queue"; queue: number | null };

export function matches(d: Download, f: Filter): boolean {
  switch (f.kind) {
    case "all":
      return true;
    case "unfinished":
      return d.status !== "finished";
    case "finished":
      return d.status === "finished";
    case "category":
      return d.category === f.category;
    case "queue":
      return (d.queue ?? null) === f.queue;
  }
}

export function sameFilter(a: Filter, b: Filter): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** What the toolbar, menu and keys may do with a download. */
export function canResume(d: Download): boolean {
  return ["queued", "paused", "failed", "interrupted"].includes(d.status);
}

/** Pause also ends a download's wait for a free slot. */
export function canPause(d: Download): boolean {
  return (d.status === "downloading" && d.resumable) || d.status === "waiting";
}

/** Running, but pausing would lose what it has (no resume support on the server). */
export function cannotPause(d: Download): boolean {
  return d.status === "downloading" && !d.resumable;
}

/** Why a running download cannot be paused, in the interface language. */
export const cannotPauseText = () => t("download.cannotPause");
