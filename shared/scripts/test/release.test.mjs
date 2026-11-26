import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { EXTENSION_ID, extensionId } from "../extension-id.mjs";
import { changelogSection, notes, FFMPEG } from "../release-notes.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "../..");

test("the manifest key gives the extension ID the native host accepts", () => {
  const { key } = JSON.parse(readFileSync(join(root, "extension/manifest.json"), "utf8"));
  assert.equal(extensionId(key), EXTENSION_ID);
  // The same ID is the one the Rust side allows.
  assert.match(readFileSync(join(root, "src-tauri/src/handoff.rs"), "utf8"), new RegExp(`"${EXTENSION_ID}"`));
});

const CL = "# Changelog\n\n## [Unreleased]\n\n## [0.2.0] - 2027-01-01\n\n- two\n\n## [0.1.0] - 2026-11-21\n\n### Added\n\n- one\n";

test("the changelog section for a version is found, and a missing one fails", () => {
  assert.equal(changelogSection(CL, "0.1.0"), "### Added\n\n- one");
  assert.equal(changelogSection(CL, "0.2.0"), "- two");
  assert.throws(() => changelogSection(CL, "0.3.0"), /no section for 0\.3\.0/);
  assert.throws(() => changelogSection(CL, "0.1"), /no section/);
});

test("a CRLF changelog gives the same sections, with LF line ends", () => {
  const crlf = CL.replace(/\n/g, "\r\n");
  assert.equal(changelogSection(crlf, "0.1.0"), "### Added\n\n- one");
  assert.equal(changelogSection(crlf, "0.2.0"), "- two");
});

test("the notes match how the files were signed", () => {
  const adhoc = notes(CL, "0.1.0", "ad-hoc");
  assert.match(adhoc, /not signed with an Apple Developer ID/);
  assert.match(adhoc, /Open Anyway/);
  assert.doesNotMatch(adhoc, /notarised by Apple/);
  assert.doesNotMatch(adhoc, /Windows/);
  assert.match(adhoc, /does not check for updates/);
  const signed = notes(CL, "0.1.0", "developer-id", "unsigned");
  assert.match(signed, /notarised by Apple/);
  assert.doesNotMatch(signed, /Open Anyway/);
  assert.match(signed, /windows-x64-setup-beta\.exe/);
  assert.throws(() => notes(CL, "0.1.0", "maybe"), /unknown macOS signing/);
  // Five release files: no SHA256SUMS; the ffmpeg source is attached and named with its
  // official URL and pinned hash.
  for (const n of [adhoc, signed]) {
    assert.doesNotMatch(n, /SHA256SUMS/);
    assert.match(n, /gh attestation verify <file> -R kumasumit\/ks-download-manager/);
    assert.ok(n.includes(`built from ffmpeg ${FFMPEG.version}. Its exact source is attached as \`ffmpeg-${FFMPEG.version}.tar.xz\` (from ${FFMPEG.source}, SHA-256 \`${FFMPEG.sha256}\`)`));
  }
  // The real changelog has the section for the first release.
  assert.ok(changelogSection(readFileSync(join(root, "..", "CHANGELOG.md"), "utf8"), "0.1.2").length > 0);
});
