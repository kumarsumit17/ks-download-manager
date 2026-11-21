import { test } from "node:test";
import assert from "node:assert/strict";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { setVersion, versionOf } from "../set-version.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "../..");
const FILES = ["package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "extension/manifest.json"];

function copy() {
  const dir = mkdtempSync(join(tmpdir(), "set-version-"));
  for (const f of FILES) {
    mkdirSync(dirname(join(dir, f)), { recursive: true });
    copyFileSync(join(root, f), join(dir, f));
  }
  return dir;
}

test("a good tag writes the version into every file", () => {
  const dir = copy();
  try {
    assert.equal(setVersion("v1.20.3", dir), "1.20.3");
    assert.equal(JSON.parse(readFileSync(join(dir, "package.json"), "utf8")).version, "1.20.3");
    assert.equal(JSON.parse(readFileSync(join(dir, "src-tauri/tauri.conf.json"), "utf8")).version, "1.20.3");
    assert.equal(JSON.parse(readFileSync(join(dir, "extension/manifest.json"), "utf8")).version, "1.20.3");
    assert.match(readFileSync(join(dir, "src-tauri/Cargo.toml"), "utf8"), /^version = "1\.20\.3"$/m);
    const lock = readFileSync(join(dir, "src-tauri/Cargo.lock"), "utf8");
    assert.match(lock, /name = "ks-download-manager"\nversion = "1\.20\.3"\n/);
    // Only our package's entry changes; the engine crate keeps its placeholder.
    assert.match(lock, /name = "engine"\nversion = "0\.0\.0"\n/);
  } finally {
    rmSync(dir, { recursive: true });
  }
});

test("bad tags are refused", () => {
  for (const tag of ["v0.1", "0.1.0", "v0.1.0-beta", "v01.0.0", "v0.01.0", "v1.2.3.4", "", undefined, "refs/tags/v1.0.0"]) {
    assert.throws(() => versionOf(tag), /not a release tag/, String(tag));
  }
  assert.equal(versionOf("v0.0.0"), "0.0.0");
  assert.equal(versionOf("v10.0.12"), "10.0.12");
});

test("a file already changed stops the script before anything is written", () => {
  const dir = copy();
  try {
    const manifest = join(dir, "extension/manifest.json");
    writeFileSync(manifest, readFileSync(manifest, "utf8").replace('"0.0.0"', '"9.9.9"'));
    const before = readFileSync(join(dir, "package.json"), "utf8");
    assert.throws(() => setVersion("v1.0.0", dir), /extension\/manifest\.json: expected the placeholder exactly once, found 0/);
    assert.equal(readFileSync(join(dir, "package.json"), "utf8"), before, "nothing written");
    // Running twice is refused too.
    const dir2 = copy();
    setVersion("v1.0.0", dir2);
    assert.throws(() => setVersion("v1.0.1", dir2), /already has a version/);
    rmSync(dir2, { recursive: true });
  } finally {
    rmSync(dir, { recursive: true });
  }
});
