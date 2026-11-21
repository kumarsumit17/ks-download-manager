// When the download list must be re-sorted, and when it must scroll to the selection.
// No imports, so `node --test` runs it directly (test/ui/order.test.ts).

/**
 * What the list's order depends on: each row's id, status, playlist and its value in the
 * sort column. While this string stays the same, the sorted and grouped order cannot
 * change, so a progress tick (new byte counts and speeds) only swaps in the fresh rows.
 * Sorting by speed or time left changes it on every tick, as it should.
 */
export function orderSignature<T extends { id: number; status: string; playlist?: { id: number } | null }>(
  rows: readonly T[],
  value: (row: T) => number | string,
): string {
  let s = "";
  for (const r of rows) s += `${r.id}\u0000${r.status}\u0000${r.playlist?.id ?? ""}\u0000${value(r)}\u0001`;
  return s;
}

/**
 * Where to scroll so the selected row shows, or `null` to stay put. Only a change of the
 * selection scrolls: progress updates, new rows and re-sorting never pull the list back
 * to it. `index` is the selected row's position (-1 when it is not in the list).
 */
export function revealSelection(
  previous: number | null,
  selected: number | null,
  index: number,
  rowHeight: number,
  scrollTop: number,
  viewport: number,
): number | null {
  if (selected === null || selected === previous || index < 0) return null;
  const top = index * rowHeight;
  const bottom = top + rowHeight;
  if (top < scrollTop) return top;
  if (bottom > scrollTop + viewport) return Math.max(0, bottom - viewport);
  return null;
}
