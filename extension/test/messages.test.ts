import { test } from "node:test";
import assert from "node:assert/strict";
import { isNewer, isSafe } from "../src/messages.ts";

test("only downloads Chrome found safe, or the user accepted, are handed over", () => {
  assert.equal(isSafe("safe"), true);
  assert.equal(isSafe("accepted"), true);
  for (const d of ["file", "url", "content", "uncommon", "host", "unwanted", "asyncScanning", "promptForScanning", "", undefined, null]) {
    assert.equal(isSafe(d), false, String(d));
  }
});

test("the popup offers an update only for a newer, plain version, and never in a copy built from source", () => {
  assert.equal(isNewer("0.1.4", "0.1.3"), true);
  assert.equal(isNewer("0.1.10", "0.1.9"), true);
  assert.equal(isNewer("1.0.0", "0.9.9"), true);
  assert.equal(isNewer("0.1.3", "0.1.3"), false);
  assert.equal(isNewer("0.1.2", "0.1.3"), false);
  assert.equal(isNewer("0.2.0", "0.0.0"), false, "built from source");
  for (const v of [undefined, null, 4, "", "v0.2.0", "0.2.0-beta", "<b>9</b>"]) assert.equal(isNewer(v, "0.1.3"), false, String(v));
});
