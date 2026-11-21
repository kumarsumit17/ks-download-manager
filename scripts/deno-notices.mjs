// Collects the licence texts of everything Deno is built from, for licenses/deno-THIRD-PARTY.txt.
//
//   node scripts/deno-notices.mjs <deno version> <output file>
//
// Reads Deno's Cargo.lock at that tag, downloads each crates.io crate's published source
// archive (cached in src-tauri/binaries/.cache/crates/), and copies every LICENSE*,
// LICENCE*, COPYING*, NOTICE* and COPYRIGHT* file in it. Also adds V8's and ICU's licences,
// which Deno links in. Development tool only; nothing here is shipped except its output.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const [version, outFile] = process.argv.slice(2);
if (!version || !outFile) throw new Error("usage: deno-notices.mjs <version> <output>");
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const cache = join(root, "src-tauri/binaries/.cache/crates");
mkdirSync(cache, { recursive: true });

const text = async (url) => {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  return r.text();
};
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
      return { name, ver, texts: [], missing: `no licence file in the published crate; declares: ${declared}` };
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
for (let i = 0; i < crates.length; i += 12) {
  results.push(...(await Promise.all(crates.slice(i, i + 12).map(licences))));
}
const v8 = await text("https://raw.githubusercontent.com/v8/v8/main/LICENSE");
const icu = await text("https://raw.githubusercontent.com/unicode-org/icu/main/LICENSE");

const missing = results.filter((r) => r.missing);
let out = `Deno ${version} (MIT, see deno-LICENSE.md) and what it is built from.\n`;
out += `Source: https://github.com/denoland/deno/tree/v${version}\n`;
out += `Crates from crates.io: ${crates.length}; with licence texts below: ${crates.length - missing.length}.\n\n`;
if (missing.length) {
  out += `Crates whose published archive has no licence file (their licence is declared in their Cargo.toml):\n`;
  out += missing.map((m) => `  ${m.name} ${m.ver}: ${m.missing}`).join("\n") + "\n\n";
}
if (others.length) out += `Other sources (not from crates.io; see their repositories):\n  ${others.join("\n  ")}\n\n`;
out += `${"=".repeat(78)}\nV8 (https://github.com/v8/v8)\n${"=".repeat(78)}\n${v8}\n`;
out += `${"=".repeat(78)}\nICU (https://github.com/unicode-org/icu)\n${"=".repeat(78)}\n${icu}\n`;
for (const r of results.filter((r) => !r.missing)) {
  for (const t of r.texts) out += `${"=".repeat(78)}\n${r.name} ${r.ver}: ${t.file}\n${"=".repeat(78)}\n${t.body.trimEnd()}\n\n`;
}
writeFileSync(outFile, out);
console.log(`${crates.length} crates, ${missing.length} without a licence file, ${others.length} other sources`);
