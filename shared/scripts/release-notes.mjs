// Writes the GitHub release notes: this release's section of CHANGELOG.md, then one block
// per platform the release has, matching how its files were actually built and signed.
//
//   node scripts/release-notes.mjs <version> <platform: all|mac|windows|chrome> [artifacts dir]
//
// The macOS signing (developer-id or ad-hoc) is read from the signing-macos-<arch>.txt files
// the mac build jobs leave in the artifacts; both builds must have been signed the same way.
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "kumasumit/ks-download-manager";
/** The workflow that signs the attestations: ks-utility's shared publish workflow. */
const SIGNER = "kumasumit/ks-utility/.github/workflows/tauri-app-release-publish.yml";
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
/** The pinned ffmpeg source: version, official URL and SHA-256 (src-tauri/sidecars.json). */
export const FFMPEG = JSON.parse(readFileSync(join(ROOT, "src-tauri/sidecars.json"), "utf8")).ffmpeg;
/** The ks-utility version this build uses: the tag its pin in package.json names. */
export const KS_UTILITY = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8")).dependencies["ks-utility"].split("#").pop();

/** The body of "## <version> …" in a Keep a Changelog file, or throws. */
export function changelogSection(text, version) {
  const lines = text.split(/\r?\n/); // a CRLF checkout reads the same
  const start = lines.findIndex((l) => new RegExp(`^## \\[?${version.replace(/\./g, "\\.")}\\]?( |$)`).test(l));
  if (start < 0) throw new Error(`CHANGELOG.md has no section for ${version}`);
  let end = lines.findIndex((l, i) => i > start && /^## /.test(l));
  if (end < 0) end = lines.length;
  return lines.slice(start + 1, end).join("\n").trim();
}

export const NAMES = { mac: "macOS", windows: "Windows", chrome: "Chrome" };

/** The changelog heading of a release: "0.2.0" for every platform, "macOS 0.2.1" for one. */
export const headingOf = (version, platform) => (platform === "all" ? version : `${NAMES[platform]} ${version}`);

/**
 * The notes for a release of `platform` (all, mac, windows or chrome). `mac` is the macOS
 * signing (developer-id or ad-hoc), needed when the release has macOS files.
 */
export function notes(changelog, version, platform, mac) {
  if (platform !== "all" && !(platform in NAMES)) throw new Error(`unknown platform "${platform}"`);
  const has = (p) => platform === "all" || platform === p;
  if (has("mac") && !["developer-id", "ad-hoc"].includes(mac)) throw new Error(`unknown macOS signing "${mac}"`);
  let out = `${changelogSection(changelog, headingOf(version, platform))}\n\n## Downloads\n\n`;
  if (has("mac")) {
    out += `### macOS 11 or later\n\n`;
    out += `- Apple silicon (M1 and later): \`ks-download-manager-v${version}-macos-arm64.dmg\`\n`;
    out += `- Intel: \`ks-download-manager-v${version}-macos-x64.dmg\`\n\n`;
    out += `Open the disk image and drag **KS Download Manager** to **Applications**.\n\n`;
    if (mac === "developer-id") {
      out += `The app is signed with a Developer ID and notarised by Apple.\n\n`;
    } else {
      out += `**This build is not signed with an Apple Developer ID** (it is signed ad hoc). The first time you open it, macOS says it cannot be opened or that it is damaged. To open it:\n\n`;
      out += `1. Try to open the app once, then choose **Done** (or **Cancel**).\n`;
      out += `2. Open **System Settings → Privacy & Security**, scroll down, and choose **Open Anyway** next to the message about KS Download Manager.\n`;
      out += `3. Confirm with your password.\n\n`;
      out += `Or, in Terminal: \`xattr -dr com.apple.quarantine "/Applications/KS Download Manager.app"\`\n\n`;
    }
  }
  if (has("windows")) {
    out += `### Windows 10 and 11 (beta, untested by hand)\n\n`;
    out += `- \`ks-download-manager-v${version}-windows-x64-setup-beta.exe\` (installs for the current user, no administrator rights)\n\n`;
    out += `The installer is not code-signed. Windows SmartScreen shows "Windows protected your PC": choose **More info → Run anyway**.\n\n`;
  }
  if (has("chrome")) {
    out += `### Chrome extension\n\n`;
    out += `- \`ks-download-manager-extension-v${version}.zip\`: unzip it to a folder you keep, open \`chrome://extensions\`, turn on **Developer mode**, choose **Load unpacked** and pick that folder. See the README.\n\n`;
  }
  out += `### Checking the files\n\n`;
  out += `Each file has a build-provenance attestation: \`gh attestation verify <file> -R ${REPO} --signer-workflow ${SIGNER}\`.\n\n`;
  if (has("mac") || has("windows")) {
    out += `The bundled ffmpeg and ffprobe are LGPL-2.1-or-later, built from ffmpeg ${FFMPEG.version}. Its exact source is attached as \`${FFMPEG.source.split("/").pop()}\` (from ${FFMPEG.source}, SHA-256 \`${FFMPEG.sha256}\`). The build details are in the app's \`licenses\` folder (\`ffmpeg-SOURCE.txt\`).\n\n`;
  }
  out += `Built with [ks-utility](https://github.com/kumasumit/ks-utility) ${KS_UTILITY}.\n\n`;
  out += `The app looks for a new version once a day (a switch in Settings turns this off). Every version is published at https://github.com/${REPO}/releases.\n`;
  return out;
}

/** The macOS signing recorded by the mac build jobs (signing-macos-<arch>.txt under `dir`). */
export function macSigning(dir) {
  const found = readdirSync(dir, { recursive: true }).map(String).filter((f) => /(^|[\\/])signing-macos-(arm64|x64)\.txt$/.test(f));
  const modes = new Set(found.map((f) => readFileSync(join(dir, f), "utf8").trim()));
  if (found.length !== 2) throw new Error(`expected signing-macos-arm64.txt and signing-macos-x64.txt, found: ${found.join(", ") || "none"}`);
  if (modes.size !== 1) throw new Error(`the two macOS builds were signed differently (${[...modes].join(", ")})`);
  return [...modes][0];
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [version, platform, artifacts] = process.argv.slice(2);
  const mac = platform === "all" || platform === "mac" ? macSigning(artifacts ?? "artifacts") : undefined;
  process.stdout.write(notes(readFileSync(join(ROOT, "..", "CHANGELOG.md"), "utf8"), version, platform, mac));
}
