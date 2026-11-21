import { test } from "node:test";
import assert from "node:assert/strict";
import { spdxIds } from "../deno-notices.mjs";

test("SPDX identifiers are read from Cargo licence values", () => {
  assert.deepEqual(spdxIds("MIT"), ["MIT"]);
  assert.deepEqual(spdxIds("MIT OR Apache-2.0"), ["MIT", "Apache-2.0"]);
  assert.deepEqual(spdxIds("MIT/Apache-2.0"), ["MIT", "Apache-2.0"]);
  assert.deepEqual(spdxIds("Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT"), ["Apache-2.0", "LLVM-exception", "MIT"]);
  assert.deepEqual(spdxIds("(MIT OR Apache-2.0) AND Unicode-3.0"), ["MIT", "Apache-2.0", "Unicode-3.0"]);
  assert.deepEqual(spdxIds("MIT OR Apache-2.0 OR LGPL-2.1-or-later"), ["MIT", "Apache-2.0", "LGPL-2.1-or-later"]);
});
