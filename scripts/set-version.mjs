// Writes the version from a release tag into the files that carry one. Run only in CI on a
// fresh checkout; the repository itself keeps the placeholder 0.0.0 (specs/versioning in the
// workspace: versions come from git tags, never hardcoded).
//
//   node scripts/set-version.mjs <tag> [root]      e.g. v1.2.3
//
// Accepts only vMAJOR.MINOR.PATCH (no leading zeros, no suffix). Every edit is checked: each
// file must still hold exactly the placeholder (or, for the JSON files without a version, no
// version key), otherwise nothing is written and the script exits 1.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const TAG = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

/** Returns "X.Y.Z" for a valid tag, or throws. */
export function versionOf(tag) {
  if (!TAG.test(tag ?? "")) throw new Error(`"${tag}" is not a release tag (expected vMAJOR.MINOR.PATCH, e.g. v1.2.3)`);
  return tag.slice(1);
}

const once = (text, find, replace, file) => {
  const n = text.split(find).length - 1;
  if (n !== 1) throw new Error(`${file}: expected the placeholder exactly once, found ${n}`);
  return text.replace(find, replace);
};

// Adds "version" right after the first key of a JSON object, keeping the file's layout.
const addJsonVersion = (text, version, after, file) => {
  if (/^\s*"version"\s*:/m.test(text)) throw new Error(`${file}: already has a version`);
  const re = new RegExp(`^(\\s*)"${after}"\\s*:.*,$`, "m");
  const m = text.match(re);
  if (!m) throw new Error(`${file}: "${after}" line not found`);
  return text.replace(re, `${m[0]}\n${m[1]}"version": "${version}",`);
};

/** The edits, as [relative path, function(text) => new text]. */
export function edits(version) {
  return [
    ["package.json", (t, f) => addJsonVersion(t, version, "private", f)],
    ["src-tauri/tauri.conf.json", (t, f) => addJsonVersion(t, version, "productName", f)],
    ["src-tauri/Cargo.toml", (t, f) => once(t, '\nversion = "0.0.0"\n', `\nversion = "${version}"\n`, f)],
    [
      "src-tauri/Cargo.lock",
      (t, f) => once(t, 'name = "ks-download-manager"\nversion = "0.0.0"\n', `name = "ks-download-manager"\nversion = "${version}"\n`, f),
    ],
    ["extension/manifest.json", (t, f) => once(t, '"version": "0.0.0"', `"version": "${version}"`, f)],
  ];
}

/** Checks every file first, then writes them all. */
export function setVersion(tag, root) {
  const version = versionOf(tag);
  const out = edits(version).map(([rel, edit]) => {
    const file = join(root, rel);
    return [file, edit(readFileSync(file, "utf8"), rel)];
  });
  for (const [file, text] of out) writeFileSync(file, text);
  return version;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [tag, root = join(dirname(fileURLToPath(import.meta.url)), "..")] = process.argv.slice(2);
  try {
    console.log(`version ${setVersion(tag, root)}`);
  } catch (e) {
    console.error(`set-version: ${e.message}`);
    process.exit(1);
  }
}
