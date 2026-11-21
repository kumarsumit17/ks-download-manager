import { useEffect, useSyncExternalStore } from "react";
import { api, onChanged, onProgress, onRemoved } from "./api";
import { type Download, type DownloadRecord, MAIN_QUEUE, type Tick } from "./types";

// A small store for the download list. The state lives in Rust; this mirrors it from
// events: a full record on every status change, and one batch of numbers a few times a
// second while downloads run.

let rows = new Map<number, Download>();
let snapshot: Download[] = [];
let loaded = false;
const listeners = new Set<() => void>();

function publish() {
  snapshot = [...rows.values()];
  for (const l of listeners) l();
}

function fromRecord(r: DownloadRecord, old?: Download): Download {
  const running = r.status === "downloading";
  return {
    ...r,
    queue: MAIN_QUEUE,
    speed: running ? (old?.speed ?? 0) : 0,
    connections: running ? (old?.connections ?? []) : [],
    merging: running ? (old?.merging ?? false) : false,
    // Keep the live count while running: the record's own count is only updated on stop.
    received: running && old ? Math.max(old.received, r.received) : r.received,
  };
}

function changed(r: DownloadRecord) {
  rows = new Map(rows).set(r.id, fromRecord(r, rows.get(r.id)));
  publish();
}

function removed(id: number) {
  if (!rows.has(id)) return;
  rows = new Map(rows);
  rows.delete(id);
  publish();
}

function progress(ticks: Tick[]) {
  let next: Map<number, Download> | null = null;
  for (const t of ticks) {
    const d = rows.get(t.id);
    if (!d || d.status !== "downloading") continue;
    next ??= new Map(rows);
    next.set(t.id, { ...d, received: t.received, size: t.size ?? d.size, speed: t.speed, connections: t.connections, merging: t.merging });
  }
  if (next) {
    rows = next;
    publish();
  }
}

let started = false;

/** Subscribes to Rust once, then loads the list. Events that arrive while it loads win. */
async function start() {
  if (started) return;
  started = true;
  const seen = new Set<number>();
  await Promise.all([
    onChanged((r) => {
      seen.add(r.id);
      changed(r);
    }),
    onRemoved((id) => {
      seen.add(id);
      removed(id);
    }),
    onProgress(progress),
  ]);
  const list = await api.list();
  const next = new Map(rows);
  for (const r of list) if (!seen.has(r.id)) next.set(r.id, fromRecord(r));
  rows = next;
  loaded = true;
  publish();
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

export function useDownloads(): { downloads: Download[]; loaded: boolean } {
  useEffect(() => {
    void start();
  }, []);
  const downloads = useSyncExternalStore(subscribe, () => snapshot);
  return { downloads, loaded: useSyncExternalStore(subscribe, () => loaded) };
}
