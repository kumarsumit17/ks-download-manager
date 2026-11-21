// Run with `npm run test:ui` (Node strips the types; nothing is installed).
import { test } from "node:test";
import assert from "node:assert/strict";
import { allAvailable, clickEntry, formatDuration, groupRows, imageType, toggleAll } from "../../src/features/downloads/playlist.ts";

const entries = [1, 2, 3, 4, 5, 6].map((index) => ({ index, unavailable: index === 4 }));

test("every available entry is ticked when the list opens", () => {
  assert.deepEqual([...allAvailable(entries)], [1, 2, 3, 5, 6]);
});

test("a click toggles one entry and a shift-click sets the range to the same state", () => {
  let s = clickEntry(entries, allAvailable(entries), null, 2, false);
  assert.deepEqual([...s.ticked].sort(), [1, 3, 5, 6]);
  s = clickEntry(entries, s.ticked, s.anchor, 6, true);
  assert.deepEqual([...s.ticked].sort(), [1], "2..6 cleared, the unavailable one skipped");
  s = clickEntry(entries, s.ticked, s.anchor, 3, true);
  assert.deepEqual([...s.ticked].sort(), [1, 3, 5, 6], "upwards too; 4 is never ticked");
  s = clickEntry(entries, new Set(), null, 5, true);
  assert.deepEqual([...s.ticked], [5], "shift without an anchor is a plain click");
});

test("select all ticks every available entry, or clears all", () => {
  assert.deepEqual([...toggleAll(entries, new Set([1]))].sort(), [1, 2, 3, 5, 6]);
  assert.equal(toggleAll(entries, allAvailable(entries)).size, 0);
});

test("durations use Latin digits", () => {
  assert.equal(formatDuration(61), "1:01");
  assert.equal(formatDuration(3725), "1:02:05");
  assert.equal(formatDuration(null), "");
  assert.equal(formatDuration(-1), "");
});

test("playlist rows are grouped where the first appears, in playlist order", () => {
  const p = (index: number) => ({ id: 9, title: "L", index });
  const rows = [
    { id: 1, playlist: p(3) },
    { id: 2, playlist: null },
    { id: 3, playlist: p(1) },
    { id: 4 },
  ];
  const g = groupRows(rows);
  assert.deepEqual(g.map((x) => x.rows.map((r) => r.id)), [[3, 1], [2], [4]]);
  assert.deepEqual(g[0].group, { id: 9, title: "L" });
  assert.equal(g[1].group, null);
});

test("thumbnails are only JPEG, PNG, WebP or GIF", () => {
  assert.equal(imageType(new Uint8Array([0xff, 0xd8, 0xff, 0xe0])), "image/jpeg");
  assert.equal(imageType(new TextEncoder().encode("RIFF\0\0\0\0WEBPVP8 ")), "image/webp");
  assert.equal(imageType(new TextEncoder().encode("<svg>")), null);
});
