import { test } from "node:test";
import assert from "node:assert/strict";
import { isSafe } from "../src/messages.ts";

test("only downloads Chrome found safe, or the user accepted, are handed over", () => {
  assert.equal(isSafe("safe"), true);
  assert.equal(isSafe("accepted"), true);
  for (const d of ["file", "url", "content", "uncommon", "host", "unwanted", "asyncScanning", "promptForScanning", "", undefined, null]) {
    assert.equal(isSafe(d), false, String(d));
  }
});
