// The 10,000-row list: only the rows on screen (plus a few) are drawn.
import { test } from "node:test";
import assert from "node:assert/strict";
import { OVERSCAN, scrollToShow, visibleRange } from "../../src/features/downloads/windowing.ts";

const ROW = 36;

test("10,000 rows draw only what fits on screen", () => {
  const view = 720; // 20 rows
  for (const scroll of [0, 1, 17_999, 180_000, 359_280, 1e9]) {
    const r = visibleRange(10_000, ROW, scroll, view);
    assert.ok(r.end - r.start <= 20 + 1 + 2 * OVERSCAN, `at ${scroll}: ${r.end - r.start} rows`);
    assert.ok(r.end <= 10_000 && r.start >= 0 && r.start < r.end);
    assert.equal(r.before + (r.end - r.start) * ROW + r.after, 10_000 * ROW, "the full height is kept");
  }
  const mid = visibleRange(10_000, ROW, 5000 * ROW, view);
  assert.equal(mid.start, 5000 - OVERSCAN);
  assert.equal(mid.end, 5020 + OVERSCAN);
});

test("an empty or short list draws everything", () => {
  assert.deepEqual(visibleRange(0, ROW, 0, 500), { start: 0, end: 0, before: 0, after: 0 });
  const r = visibleRange(3, ROW, 0, 500);
  assert.deepEqual([r.start, r.end, r.after], [0, 3, 0]);
});

test("the selected row is scrolled into view only when needed", () => {
  assert.equal(scrollToShow(10, ROW, 0, 720), null);
  assert.equal(scrollToShow(30, ROW, 0, 720), 31 * ROW - 720);
  assert.equal(scrollToShow(2, ROW, 10 * ROW, 720), 2 * ROW);
});
