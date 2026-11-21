// Fetches, checks and lays out the programs the app bundles (specs/video-playlists.md,
// "Bundled programs"): yt-dlp (official release, unmodified), Deno (official release)
// and ffmpeg/ffprobe (built from the pinned source by scripts/build-ffmpeg.sh).
//
//   node scripts/sidecars.mjs [target-triple]     (default: this Mac's)
//
// Every download is checked against the SHA-256 pinned in src-tauri/sidecars.json before
// it is unpacked or used; a mismatch stops the script. Output goes to src-tauri/binaries/
// (git-ignored), where tauri.conf.json picks it up. Runs again quickly: finished steps
// are skipped while their stamp file matches the pinned hash.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const pins = JSON.parse(readFileSync(join(root, "src-tauri/sidecars.json"), "utf8"));
const out = join(root, "src-tauri/binaries");
const cache = join(out, ".cache");
const licences = join(out, "licenses");
const triple =
  process.argv[2] ?? (process.arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin");
if (!triple.endsWith("apple-darwin")) {
  throw new Error(`${triple}: only macOS targets are set up so far (Windows is the Release stage)`);
}
const arch = triple.startsWith("aarch64") ? "arm64" : "x86_64";
mkdirSync(cache, { recursive: true });
mkdirSync(licences, { recursive: true });

const sha256 = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");

async function fetchTo(url, file) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  writeFileSync(file, Buffer.from(await res.arrayBuffer()));
}

async function fetchPinned(url, hash, name) {
  const file = join(cache, name);
  if (!existsSync(file) || sha256(file) !== hash) await fetchTo(url, file);
  const got = sha256(file);
  if (got !== hash) {
    rmSync(file);
    throw new Error(`${name}: SHA-256 ${got} does not match the pinned ${hash}`);
  }
  return file;
}

const stamp = (name) => join(out, `.${name}.stamp`);
const done = (name, hash) => existsSync(stamp(name)) && readFileSync(stamp(name), "utf8") === hash;
const mark = (name, hash) => writeFileSync(stamp(name), hash);

// yt-dlp: the unpacked ("onedir") macOS build, so nothing is unpacked to a temp folder
// at every run and every library can be signed with the app.
{
  const { version, macos } = pins["yt-dlp"];
  if (!done("yt-dlp", macos.sha256)) {
    const zip = await fetchPinned(macos.url, macos.sha256, `yt-dlp_macos-${version}.zip`);
    rmSync(join(out, "yt-dlp"), { recursive: true, force: true });
    execFileSync("ditto", ["-x", "-k", zip, join(out, "yt-dlp")]);
    await fetchTo(`https://raw.githubusercontent.com/yt-dlp/yt-dlp/${version}/LICENSE`, join(licences, "yt-dlp-LICENSE.txt"));
    mark("yt-dlp", macos.sha256);
  }
}

// Deno: the JavaScript runtime yt-dlp needs for YouTube.
{
  const { version } = pins.deno;
  const pin = pins.deno[triple];
  if (!done(`deno-${triple}`, pin.sha256)) {
    const zip = await fetchPinned(pin.url, pin.sha256, `deno-${version}-${triple}.zip`);
    const tmp = join(cache, `deno-${triple}`);
    rmSync(tmp, { recursive: true, force: true });
    execFileSync("ditto", ["-x", "-k", zip, tmp]);
    copyFileSync(join(tmp, "deno"), join(out, `deno-${triple}`));
    // Deno's own licence, and the list of the Rust crates built into it at this tag.
    const raw = `https://raw.githubusercontent.com/denoland/deno/v${version}`;
    await fetchTo(`${raw}/LICENSE.md`, join(licences, "deno-LICENSE.md"));
    // Licence texts of everything Deno is built from (scripts/deno-notices.mjs).
    execFileSync("node", [join(root, "scripts/deno-notices.mjs"), version, join(licences, "deno-THIRD-PARTY.txt")], { stdio: "inherit" });
    mark(`deno-${triple}`, pin.sha256);
  }
}

// ffmpeg + ffprobe, LGPL, built from the pinned source tarball.
{
  const { version, source, sha256: hash } = pins.ffmpeg;
  if (!done(`ffmpeg-${triple}`, hash)) {
    const work = join(cache, "ffmpeg");
    mkdirSync(work, { recursive: true });
    await fetchPinned(source, hash, join("ffmpeg", `ffmpeg-${version}.tar.xz`));
    execFileSync(join(root, "scripts/build-ffmpeg.sh"), [work, arch], { stdio: "inherit" });
    for (const tool of ["ffmpeg", "ffprobe"]) copyFileSync(join(work, `out-${arch}`, tool), join(out, `${tool}-${triple}`));
    copyFileSync(join(work, `out-${arch}`, "COPYING.LGPLv2.1"), join(licences, "ffmpeg-COPYING.LGPLv2.1"));
    copyFileSync(join(work, `out-${arch}`, "LICENSE.md"), join(licences, "ffmpeg-LICENSE.md"));
    writeFileSync(
      join(licences, "ffmpeg-SOURCE.txt"),
      `ffmpeg ${version}, LGPL-2.1-or-later.\nSource: ${source}\nSHA-256: ${hash}\n` +
        `Built with scripts/build-ffmpeg.sh from the KS Download Manager repository; the configure\n` +
        `line there is the complete build configuration. The same tarball is attached to each release.\n`,
    );
    mark(`ffmpeg-${triple}`, hash);
  }
}
console.log(`sidecars ready for ${triple} in src-tauri/binaries/`);
