import { test } from "node:test";
import assert from "node:assert/strict";
import { browserHeaders, isDangerous, isNewer, MAX_COOKIE } from "../src/messages.ts";

test("downloads Chrome flags stay in Chrome; safe, kept and not-yet-rated ones are handed over", () => {
  for (const d of ["safe", "accepted", "allowlistedByPolicy", "deepScannedSafe", "", undefined, null]) {
    assert.equal(isDangerous(d), false, String(d));
  }
  for (const d of ["file", "url", "content", "uncommon", "host", "unwanted", "asyncScanning", "promptForScanning", "sensitiveContentBlock"]) {
    assert.equal(isDangerous(d), true, d);
  }
});

test("the browser's cookies, referring page and identification go with a handed-over download", () => {
  const h = browserHeaders([{ name: "s", value: "1" }, { name: "t", value: "two" }], "https://example.com/page", "Mozilla/5.0 Test");
  assert.deepEqual(h, { cookie: "s=1; t=two", referer: "https://example.com/page", userAgent: "Mozilla/5.0 Test" });
  assert.deepEqual(browserHeaders([], undefined, undefined), {}, "nothing known, nothing sent");
  assert.equal(browserHeaders([], "javascript:alert(1)", "x").referer, undefined, "only an http(s) page");
  assert.equal(browserHeaders([{ name: "a", value: "x".repeat(MAX_COOKIE) }], "", "x").cookie, undefined, "too long for the app");
  assert.equal(browserHeaders([{ name: "a", value: "é" }], "", "x").cookie, undefined, "not sendable as a header");
  assert.equal(browserHeaders([], "", "x".repeat(600)).userAgent, undefined);
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
