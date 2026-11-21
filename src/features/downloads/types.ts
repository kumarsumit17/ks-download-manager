export type Category = "video" | "audio" | "images" | "other";
export type Status = "downloading" | "paused" | "queued" | "finished" | "failed";

export interface Download {
  id: string;
  name: string;
  url: string;
  category: Category;
  status: Status;
  /** Total size in bytes; null when the server did not say. */
  size: number | null;
  received: number;
  /** Bytes per second; 0 when not downloading. */
  speed: number;
  connections: number;
  queue: string;
  addedAt: number;
}

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
