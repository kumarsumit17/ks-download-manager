// The download list's windowing (specs/ui.md, "Performance rules"): every row has the same
// height, so which rows are on screen follows from the scroll position alone. No imports,
// so `node --test` runs it directly (test/ui/windowing.test.ts).

/** Rows drawn above and below the visible ones, so fast scrolling shows no gap. */
export const OVERSCAN = 8;

/**
 * The rows to draw: `[start, end)` of `count` rows `rowHeight` pixels high, for a viewport
 * of `viewport` pixels scrolled to `scrollTop` (both measured below the sticky header).
 * `before` and `after` are the heights of the empty space standing in for the rest.
 */
export function visibleRange(
  count: number,
  rowHeight: number,
  scrollTop: number,
  viewport: number,
  overscan = OVERSCAN,
): { start: number; end: number; before: number; after: number } {
  if (count <= 0 || rowHeight <= 0) return { start: 0, end: 0, before: 0, after: 0 };
  const top = Math.max(0, scrollTop);
  const first = Math.min(count, Math.floor(top / rowHeight));
  const last = Math.min(count, Math.ceil((top + Math.max(0, viewport)) / rowHeight));
  const start = Math.max(0, first - overscan);
  const end = Math.min(count, Math.max(last, first + 1) + overscan);
  return { start, end, before: start * rowHeight, after: (count - end) * rowHeight };
}
