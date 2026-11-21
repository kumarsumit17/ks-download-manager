// Collects the licence texts of everything Deno is built from, for licenses/deno-THIRD-PARTY.txt.
//
//   node scripts/deno-notices.mjs <deno version> <output file>
//
// Reads Deno's Cargo.lock at that tag, downloads each crates.io crate's published source
// archive (cached in src-tauri/binaries/.cache/crates/), and copies every LICENSE*,
// LICENCE*, COPYING*, NOTICE* and COPYRIGHT* file in it. Also adds V8's and ICU's licences,
// which Deno links in. Crates whose archive has no licence file get the standard text of
// each licence they declare (SPDX license-list-data, pinned below), marked as such.
// Development tool only; nothing here is shipped except its output.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// Standard licence texts: SPDX license-list-data v3.29.0, by commit so it cannot change.
export const SPDX_TAG = "v3.29.0";
const SPDX_COMMIT = "31ba1a50e5397e00a304dbadc76531740e89ee48";

/** The SPDX licence and exception identifiers in a Cargo `license` value ("MIT/Apache-2.0" is the old form of OR). */
export function spdxIds(expr) {
  return [...new Set(expr.split(/\s+(?:OR|AND|WITH)\s+|[()/]/).map((x) => x.trim()).filter((x) => /^[A-Za-z0-9.+-]+$/.test(x)))];
}

const isMain = process.argv[1] === fileURLToPath(import.meta.url);
const [version, outFile] = process.argv.slice(2);
if (isMain && (!version || !outFile)) throw new Error("usage: deno-notices.mjs <version> <output>");
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const cache = join(root, "src-tauri/binaries/.cache/crates");

const text = async (url) => {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  return r.text();
};
async function main() {
mkdirSync(cache, { recursive: true });
const lock = await text(`https://raw.githubusercontent.com/denoland/deno/v${version}/Cargo.lock`);
const crates = [];
const others = [];
for (const block of lock.split("[[package]]").slice(1)) {
  const name = block.match(/name = "([^"]+)"/)?.[1];
  const ver = block.match(/version = "([^"]+)"/)?.[1];
  const source = block.match(/source = "([^"]+)"/)?.[1];
  if (!name || !ver) continue;
  if (source?.startsWith("registry+https://github.com/rust-lang/crates.io-index")) crates.push({ name, ver });
  else if (source) others.push(`${name} ${ver} (${source})`);
  // No source: Deno's own workspace crates, under Deno's MIT licence.
}

const LICENCE_FILE = /^(licen[cs]e|copying|notice|copyright)/i;
async function licences({ name, ver }) {
  const file = join(cache, `${name}-${ver}.crate`);
  if (!existsSync(file)) {
    const r = await fetch(`https://static.crates.io/crates/${name}/${name}-${ver}.crate`);
    if (!r.ok) return { name, ver, texts: [], missing: `download failed (HTTP ${r.status})` };
    writeFileSync(file, Buffer.from(await r.arrayBuffer()));
  }
  const dir = mkdtempSync(join(tmpdir(), "crate-"));
  try {
    // Only the top folder's licence files are wanted; list first, extract those alone.
    const list = execFileSync("tar", ["-tzf", file], { encoding: "utf8", maxBuffer: 64 << 20 }).split("\n");
    const wanted = list.filter((p) => {
      const parts = p.split("/");
      return parts.length === 2 && LICENCE_FILE.test(parts[1]) && !p.includes("..");
    });
    if (wanted.length === 0) {
      const toml = list.find((p) => p.split("/").length === 2 && p.endsWith("/Cargo.toml"));
      let declared = "not declared";
      if (toml) {
        const body = execFileSync("tar", ["-xzOf", file, toml], { encoding: "utf8" });
        declared = body.match(/^license\s*=\s*"([^"]+)"/m)?.[1] ?? (body.match(/^license-file/m) ? "licence file named in Cargo.toml" : "not declared");
      }
      return { name, ver, texts: [], declared, missing: `no licence file in the published crate; declares: ${declared}` };
    }
    execFileSync("tar", ["-xzf", file, "-C", dir, ...wanted]);
    const texts = wanted
      .map((p) => join(dir, p))
      .filter((p) => statSync(p).isFile())
      .map((p) => ({ file: p.split("/").pop(), body: readFileSync(p, "utf8") }));
    return { name, ver, texts };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const results = [];
const missing = [];
for (let i = 0; i < crates.length; i += 12) {
  results.push(...(await Promise.all(crates.slice(i, i + 12).map(licences))));
}
missing.push(...results.filter((r) => r.missing));
const v8 = await text("https://raw.githubusercontent.com/v8/v8/main/LICENSE");
const icu = await text("https://raw.githubusercontent.com/unicode-org/icu/main/LICENSE");

// Standard texts for what the crates without a licence file declare, once per licence.
const users = new Map(); // SPDX id -> ["name version", …]
for (const m of missing) {
  if (!m.declared || m.declared === "not declared" || m.declared.startsWith("licence file")) continue;
  for (const id of spdxIds(m.declared)) users.set(id, [...(users.get(id) ?? []), `${m.name} ${m.ver}`]);
}
const spdxCache = join(cache, "..", "spdx", SPDX_TAG);
mkdirSync(spdxCache, { recursive: true });
const standard = [];
for (const id of [...users.keys()].sort()) {
  const file = join(spdxCache, `${id}.txt`);
  if (!existsSync(file)) writeFileSync(file, await text(`https://raw.githubusercontent.com/spdx/license-list-data/${SPDX_COMMIT}/text/${id}.txt`));
  standard.push({ id, crates: users.get(id), body: readFileSync(file, "utf8") });
}

let out = `Deno ${version} (MIT, see deno-LICENSE.md) and what it is built from.\n`;
out += `Source: https://github.com/denoland/deno/tree/v${version}\n`;
out += `Crates from crates.io: ${crates.length}; with licence texts below: ${crates.length - missing.length}.\n\n`;
if (missing.length) {
  out += `Crates whose published archive has no licence file (their licence is declared in their Cargo.toml):\n`;
  out += missing.map((m) => `  ${m.name} ${m.ver}: ${m.missing}`).join("\n") + "\n\n";
  out += `For these crates, the section "Standard licence texts" below gives the standard text of each\n`;
  out += `licence they declare. Those texts come from the declared licence (SPDX license-list-data\n`;
  out += `${SPDX_TAG}), NOT from the crate's own file: the crate publishes none, so its own copyright\n`;
  out += `notice is not known here, and placeholders such as <year> <copyright holders> are the template's.\n\n`;
}
if (others.length) out += `Other sources (not from crates.io; see their repositories):\n  ${others.join("\n  ")}\n\n`;
out += `${"=".repeat(78)}\nV8 (https://github.com/v8/v8)\n${"=".repeat(78)}\n${v8}\n`;
out += `${"=".repeat(78)}\nICU (https://github.com/unicode-org/icu)\n${"=".repeat(78)}\n${icu}\n`;
for (const r of results.filter((r) => !r.missing)) {
  for (const t of r.texts) out += `${"=".repeat(78)}\n${r.name} ${r.ver}: ${t.file}\n${"=".repeat(78)}\n${t.body.trimEnd()}\n\n`;
}
if (standard.length) {
  out += `${"#".repeat(78)}\nStandard licence texts\n${"#".repeat(78)}\n`;
  out += `The texts below are the standard text of each licence declared by the crates listed above\n`;
  out += `that publish no licence file. They come from the declared licence (SPDX license-list-data\n`;
  out += `${SPDX_TAG}, https://github.com/spdx/license-list-data), not from the crates' own files.\n\n`;
  for (const s of standard) {
    out += `${"=".repeat(78)}\n${s.id} (standard text, from the declared licence)\nDeclared by: ${s.crates.join(", ")}\n${"=".repeat(78)}\n${s.body.trimEnd()}\n\n`;
  }
}
writeFileSync(outFile, out);
console.log(`${crates.length} crates, ${missing.length} without a licence file (${standard.length} standard texts added), ${others.length} other sources`);
}

if (isMain) await main();
