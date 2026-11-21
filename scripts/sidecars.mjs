// Fetches, checks and lays out the programs the app bundles (specs/video-playlists.md,
// "Bundled programs"): yt-dlp (official release, unmodified), Deno (official release)
// and ffmpeg/ffprobe (built from the pinned source by scripts/build-ffmpeg.sh).
//
//   node scripts/sidecars.mjs [target-triple] [--ffmpeg-from <dir>]   (default: this Mac's)
//
// Targets: aarch64-apple-darwin, x86_64-apple-darwin (on a Mac), x86_64-pc-windows-msvc
// (on Windows). Windows cannot build ffmpeg itself: it takes the output folder of
// `scripts/build-ffmpeg.sh <work> windows-x86_64`, cross-built on Linux, with --ffmpeg-from.
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
const args = process.argv.slice(2);
const fromAt = args.indexOf("--ffmpeg-from");
const ffmpegFrom = fromAt >= 0 ? args.splice(fromAt, 2)[1] : undefined;
if (fromAt >= 0 && !ffmpegFrom) throw new Error("--ffmpeg-from needs a folder");
const triple = args[0] ?? (process.arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin");
const windows = triple === "x86_64-pc-windows-msvc";
if (!windows && !triple.endsWith("apple-darwin")) throw new Error(`${triple}: not a target of this app`);
if (windows && process.platform !== "win32") throw new Error(`${triple}: run this on Windows`);
if (windows && !ffmpegFrom) throw new Error(`${triple}: pass --ffmpeg-from <dir> (see the top of this file)`);
const arch = windows ? "windows-x86_64" : triple.startsWith("aarch64") ? "arm64" : "x86_64";
const exe = windows ? ".exe" : "";
// Zip files: macOS's ditto; Windows' own tar (bsdtar, in System32) reads zip files too. Named
// by its full path: Git Bash puts GNU tar first in PATH, which cannot read zip files.
const unzip = (zip, into) => {
  mkdirSync(into, { recursive: true });
  if (process.platform === "win32") execFileSync(join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe"), ["-xf", zip, "-C", into]);
  else execFileSync("ditto", ["-x", "-k", zip, into]);
};
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

// yt-dlp: the unpacked ("onedir") build, so nothing is unpacked to a temp folder at every
// run and, on macOS, every library can be signed with the app.
{
  const { version } = pins["yt-dlp"];
  const pin = pins["yt-dlp"][windows ? "windows" : "macos"];
  if (!done("yt-dlp", pin.sha256)) {
    const zip = await fetchPinned(pin.url, pin.sha256, windows ? `yt-dlp_win-${version}.zip` : `yt-dlp_macos-${version}.zip`);
    rmSync(join(out, "yt-dlp"), { recursive: true, force: true });
    unzip(zip, join(out, "yt-dlp"));
    await fetchTo(`https://raw.githubusercontent.com/yt-dlp/yt-dlp/${version}/LICENSE`, join(licences, "yt-dlp-LICENSE.txt"));
    mark("yt-dlp", pin.sha256);
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
    unzip(zip, tmp);
    copyFileSync(join(tmp, `deno${exe}`), join(out, `deno-${triple}${exe}`));
    // Deno's own licence, and the list of the Rust crates built into it at this tag.
    const raw = `https://raw.githubusercontent.com/denoland/deno/v${version}`;
    await fetchTo(`${raw}/LICENSE.md`, join(licences, "deno-LICENSE.md"));
    // Licence texts of everything Deno is built from (scripts/deno-notices.mjs).
    execFileSync("node", [join(root, "scripts/deno-notices.mjs"), version, join(licences, "deno-THIRD-PARTY.txt")], { stdio: "inherit" });
    mark(`deno-${triple}`, pin.sha256);
  }
}

// ffmpeg + ffprobe, LGPL, built from the pinned source tarball. With --ffmpeg-from the
// folder is copied every time (no stamp): it is a fresh build output each run.
{
  const { version, source, sha256: hash } = pins.ffmpeg;
  if (ffmpegFrom || !done(`ffmpeg-${triple}`, hash)) {
    let built = ffmpegFrom;
    if (!built) {
      const work = join(cache, "ffmpeg");
      mkdirSync(work, { recursive: true });
      await fetchPinned(source, hash, join("ffmpeg", `ffmpeg-${version}.tar.xz`));
      execFileSync(join(root, "scripts/build-ffmpeg.sh"), [work, arch], { stdio: "inherit" });
      built = join(work, `out-${arch}`);
    }
    for (const tool of ["ffmpeg", "ffprobe"]) copyFileSync(join(built, `${tool}${exe}`), join(out, `${tool}-${triple}${exe}`));
    copyFileSync(join(built, "COPYING.LGPLv2.1"), join(licences, "ffmpeg-COPYING.LGPLv2.1"));
    copyFileSync(join(built, "LICENSE.md"), join(licences, "ffmpeg-LICENSE.md"));
    let text =
      `ffmpeg ${version}, LGPL-2.1-or-later.\nSource: ${source}\nSHA-256: ${hash}\n` +
      `Built with scripts/build-ffmpeg.sh from the KS Download Manager repository; the configure\n` +
      `line there is the complete build configuration. The same tarball is attached to each release\n` +
      `of KS Download Manager, and can be checked against the SHA-256 above.\n`;
    if (windows) {
      // zlib is linked into the Windows programs, with the mingw-w64 start-up code.
      copyFileSync(join(built, "zlib-LICENSE.txt"), join(licences, "zlib-LICENSE.txt"));
      copyFileSync(join(built, "mingw-w64-runtime-COPYING.txt"), join(licences, "mingw-w64-runtime-COPYING.txt"));
      text += "\n" + readFileSync(join(built, "BUILD.txt"), "utf8");
    }
    writeFileSync(join(licences, "ffmpeg-SOURCE.txt"), text);
    if (!ffmpegFrom) mark(`ffmpeg-${triple}`, hash);
  }
}
console.log(`sidecars ready for ${triple} in src-tauri/binaries/`);
