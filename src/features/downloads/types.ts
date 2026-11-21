export type Category = "video" | "audio" | "images" | "other";
export type Kind = "file" | "video";
export type Status = "queued" | "downloading" | "paused" | "finished" | "failed" | "interrupted";

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
  error: string | null;
  /** What can be offered for the error: "videoOutdated" shows the update action. */
  errorCode: string | null;
  addedAt: number;
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
  /** Every download is in the main queue until queues arrive (Stage 5). */
  queue: string;
}

export interface ProbeView {
  kind: Kind;
  name: string;
  size: number | null;
  resumable: boolean;
  category: Category;
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

export const MAIN_QUEUE = "Main queue";

export type Filter =
  | { kind: "all" }
  | { kind: "unfinished" }
  | { kind: "finished" }
  | { kind: "category"; category: Category }
  | { kind: "queue"; queue: string };

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
      return d.queue === f.queue;
  }
}

export function sameFilter(a: Filter, b: Filter): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** What the toolbar, menu and keys may do with a download. */
export function canResume(d: Download): boolean {
  return ["queued", "paused", "failed", "interrupted"].includes(d.status);
}

export function canPause(d: Download): boolean {
  return d.status === "downloading" && d.resumable;
}

/** Running, but pausing would lose what it has (no resume support on the server). */
export function cannotPause(d: Download): boolean {
  return d.status === "downloading" && !d.resumable;
}

export const CANNOT_PAUSE =
  "This server does not support resuming, so this download cannot be paused or resumed. Stopping it loses what it has downloaded.";
