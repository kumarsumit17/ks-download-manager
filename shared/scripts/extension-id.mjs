// Prints the Chrome extension ID that a manifest's "key" gives: SHA-256 of the DER public
// key, first 32 hex digits, each mapped 0-f -> a-p. The native host only accepts this ID,
// so the release checks the zipped manifest against it.
//
//   node scripts/extension-id.mjs <manifest.json> [expected id]   (exit 1 if it differs)
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

export const EXTENSION_ID = "ikplfdbdfikcokpegkapjdedlgifmkbe";
/** The DEV extension's ID (extension/dev.json), allowed only by the DEV app. */
export const DEV_EXTENSION_ID = "mhgmalnfheamcgcongjgdmepagonmmbc";

export function extensionId(key) {
  const hex = createHash("sha256").update(Buffer.from(key, "base64")).digest("hex").slice(0, 32);
  return [...hex].map((c) => String.fromCharCode(97 + parseInt(c, 16))).join("");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [file, expected] = process.argv.slice(2);
  const { key } = JSON.parse(readFileSync(file, "utf8"));
  if (!key) {
    console.error(`${file}: no "key"`);
    process.exit(1);
  }
  const id = extensionId(key);
  console.log(id);
  if (expected && id !== expected) {
    console.error(`${file}: extension ID ${id}, expected ${expected}`);
    process.exit(1);
  }
}
