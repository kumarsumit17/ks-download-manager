// Playlist helpers with no imports, so `node --test` can run them directly
// (test/ui/playlist.test.ts): the picker's selection, the list's grouping, durations.

/** The picker's ticks: every available entry is ticked when the list opens. */
export function allAvailable(entries: readonly { index: number; unavailable: boolean }[]): Set<number> {
  return new Set(entries.filter((e) => !e.unavailable).map((e) => e.index));
}

/**
 * A click on entry `index`'s box. A plain click toggles it and makes it the anchor; a
 * shift-click sets every available entry from the anchor to it (inclusive, either
 * direction) to the state the clicked box gets. Unavailable entries never get ticked.
 * Returns the new ticks and anchor.
 */
export function clickEntry(
  entries: readonly { index: number; unavailable: boolean }[],
  ticked: ReadonlySet<number>,
  anchor: number | null,
  index: number,
  shift: boolean,
): { ticked: Set<number>; anchor: number } {
  const next = new Set(ticked);
  const on = !ticked.has(index);
  const at = entries.findIndex((e) => e.index === index);
  const from = shift && anchor !== null ? entries.findIndex((e) => e.index === anchor) : -1;
  const [a, b] = from < 0 ? [at, at] : [Math.min(from, at), Math.max(from, at)];
  for (let i = a; i <= b; i++) {
    const e = entries[i];
    if (!e || e.unavailable) continue;
    if (on) next.add(e.index);
    else next.delete(e.index);
  }
  return { ticked: next, anchor: index };
}

/** "Select all": ticks every available entry, or clears all when all are ticked. */
export function toggleAll(entries: readonly { index: number; unavailable: boolean }[], ticked: ReadonlySet<number>): Set<number> {
  const all = allAvailable(entries);
  const every = [...all].every((i) => ticked.has(i));
  return every ? new Set() : all;
}

/** A duration in seconds as `m:ss` or `h:mm:ss` (Latin digits, as the style guide asks). */
export function formatDuration(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds) || seconds < 0) return "";
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

export interface Grouped<T> {
  /** Set for a playlist's rows: they are shown under one heading. */
  group: { id: number; title: string } | null;
  rows: T[];
}

/**
 * Groups already-sorted rows: each playlist's rows are gathered where its first row
 * appears in the sorted order, and kept in the playlist's own order within the group.
 * Rows of no playlist stay where they are, each in a group of its own.
 */
export function groupRows<T extends { playlist?: { id: number; title: string; index: number } | null }>(sorted: readonly T[]): Grouped<T>[] {
  const out: Grouped<T>[] = [];
  const byId = new Map<number, Grouped<T>>();
  for (const r of sorted) {
    const p = r.playlist;
    if (!p) {
      out.push({ group: null, rows: [r] });
      continue;
    }
    let g = byId.get(p.id);
    if (!g) {
      g = { group: { id: p.id, title: p.title }, rows: [] };
      byId.set(p.id, g);
      out.push(g);
    }
    g.rows.push(r);
  }
  for (const g of byId.values()) g.rows.sort((a, b) => (a.playlist?.index ?? 0) - (b.playlist?.index ?? 0));
  return out;
}

/** A thumbnail's image type from its first bytes (Rust checks the same before sending). */
export function imageType(b: Uint8Array): string | null {
  const starts = (sig: number[], at = 0) => sig.every((x, i) => b[at + i] === x);
  if (starts([0xff, 0xd8, 0xff])) return "image/jpeg";
  if (starts([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) return "image/png";
  if (starts([0x52, 0x49, 0x46, 0x46]) && starts([0x57, 0x45, 0x42, 0x50], 8)) return "image/webp";
  if (starts([0x47, 0x49, 0x46, 0x38])) return "image/gif";
  return null;
}
