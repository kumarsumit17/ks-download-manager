import { test } from "node:test";
import assert from "node:assert/strict";
import { orderSignature, revealSelection } from "../../src/features/downloads/order.ts";

type Row = { id: number; status: string; name: string; speed: number; received: number; playlist?: { id: number } | null };
const rows: Row[] = [
  { id: 1, status: "downloading", name: "b", speed: 10, received: 5 },
  { id: 2, status: "paused", name: "a", speed: 0, received: 7 },
];

test("a progress tick does not change the order when sorting by name", () => {
  const before = orderSignature(rows, (r) => r.name);
  const tick = rows.map((r) => ({ ...r, speed: r.speed + 99, received: r.received + 1000 }));
  assert.equal(orderSignature(tick, (r) => r.name), before);
});

test("a status, a playlist, a new row or the sort column's value changes it", () => {
  const before = orderSignature(rows, (r) => r.name);
  assert.notEqual(orderSignature([{ ...rows[0], status: "finished" }, rows[1]], (r) => r.name), before);
  assert.notEqual(orderSignature([{ ...rows[0], playlist: { id: 9 } }, rows[1]], (r) => r.name), before);
  assert.notEqual(orderSignature([...rows, { ...rows[1], id: 3 }], (r) => r.name), before);
  const bySpeed = orderSignature(rows, (r) => r.speed);
  assert.notEqual(orderSignature([{ ...rows[0], speed: 11 }, rows[1]], (r) => r.speed), bySpeed, "sorting by speed follows it");
});

test("only a change of selection scrolls to it", () => {
  // Row 30 is off screen (20 rows of 36 px visible).
  assert.equal(revealSelection(5, 5, 30, 36, 0, 720), null, "same selection: a tick never pulls the list back");
  assert.equal(revealSelection(5, 6, 30, 36, 0, 720), 31 * 36 - 720);
  assert.equal(revealSelection(null, 6, 2, 36, 360, 720), 72, "above the view");
  assert.equal(revealSelection(5, 6, 3, 36, 0, 720), null, "already visible");
  assert.equal(revealSelection(5, null, 3, 36, 0, 720), null);
  assert.equal(revealSelection(5, 6, -1, 36, 0, 720), null, "not in the list");
});
