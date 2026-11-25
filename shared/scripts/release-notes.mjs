// Writes the GitHub release notes: this version's section of CHANGELOG.md, then one block
// per platform that matches how its files were actually built and signed.
//
//   node scripts/release-notes.mjs <version> <macos signing: developer-id|ad-hoc> [windows: none|unsigned]
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "kumarsumit17/ks-download-manager";
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
/** The pinned ffmpeg source: version, official URL and SHA-256 (src-tauri/sidecars.json). */
export const FFMPEG = JSON.parse(readFileSync(join(ROOT, "src-tauri/sidecars.json"), "utf8")).ffmpeg;

/** The body of "## <version> …" in a Keep a Changelog file, or throws. */
export function changelogSection(text, version) {
  const lines = text.split(/\r?\n/); // a CRLF checkout reads the same
  const start = lines.findIndex((l) => new RegExp(`^## \\[?${version.replace(/\./g, "\\.")}\\]?( |$)`).test(l));
  if (start < 0) throw new Error(`CHANGELOG.md has no section for ${version}`);
  let end = lines.findIndex((l, i) => i > start && /^## /.test(l));
  if (end < 0) end = lines.length;
  return lines.slice(start + 1, end).join("\n").trim();
}

export function notes(changelog, version, mac, windows = "none") {
  if (!["developer-id", "ad-hoc"].includes(mac)) throw new Error(`unknown macOS signing "${mac}"`);
  let out = `${changelogSection(changelog, version)}\n\n## Downloads\n\n`;
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
  if (windows === "unsigned") {
    out += `### Windows 10 and 11 (beta, untested by hand)\n\n`;
    out += `- \`ks-download-manager-v${version}-windows-x64-setup-beta.exe\` (installs for the current user, no administrator rights)\n\n`;
    out += `The installer is not code-signed. Windows SmartScreen shows "Windows protected your PC": choose **More info → Run anyway**.\n\n`;
  }
  out += `### Chrome extension\n\n`;
  out += `- \`ks-download-manager-extension-v${version}.zip\`: unzip it to a folder you keep, open \`chrome://extensions\`, turn on **Developer mode**, choose **Load unpacked** and pick that folder. See the README.\n\n`;
  out += `### Checking the files\n\n`;
  out += `Each file has a build-provenance attestation: \`gh attestation verify <file> -R ${REPO}\`.\n\n`;
  out += `The bundled ffmpeg and ffprobe are LGPL-2.1-or-later, built from ffmpeg ${FFMPEG.version}. Its exact source is attached as \`${FFMPEG.source.split("/").pop()}\` (from ${FFMPEG.source}, SHA-256 \`${FFMPEG.sha256}\`). The build details are in the app's \`licenses\` folder (\`ffmpeg-SOURCE.txt\`).\n\n`;
  out += `The app does not check for updates. New versions are published at https://github.com/${REPO}/releases.\n`;
  return out;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [version, mac, windows] = process.argv.slice(2);
  process.stdout.write(notes(readFileSync(join(ROOT, "..", "CHANGELOG.md"), "utf8"), version, mac, windows));
}
