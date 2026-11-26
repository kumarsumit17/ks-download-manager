import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { EXTENSION_ID, extensionId } from "../extension-id.mjs";
import { changelogSection, headingOf, macSigning, notes, FFMPEG } from "../release-notes.mjs";

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
  const adhoc = notes(CL, "0.1.0", "all", "ad-hoc");
  assert.match(adhoc, /not signed with an Apple Developer ID/);
  assert.match(adhoc, /Open Anyway/);
  assert.doesNotMatch(adhoc, /notarised by Apple/);
  assert.match(adhoc, /once a day/);
  const signed = notes(CL, "0.1.0", "all", "developer-id");
  assert.match(signed, /notarised by Apple/);
  assert.doesNotMatch(signed, /Open Anyway/);
  assert.throws(() => notes(CL, "0.1.0", "all", "maybe"), /unknown macOS signing/);
  assert.throws(() => notes(CL, "0.1.0", "linux"), /unknown platform/);
  for (const n of [adhoc, signed]) {
    assert.match(n, /windows-x64-setup-beta\.exe/);
    assert.match(n, /extension-v0\.1\.0\.zip/);
    assert.doesNotMatch(n, /SHA256SUMS/);
    assert.match(n, /gh attestation verify <file> -R kumasumit\/ks-download-manager --signer-workflow kumasumit\/ks-utility\/\.github\/workflows\/tauri-app-release-publish\.yml/);
    assert.ok(n.includes(`built from ffmpeg ${FFMPEG.version}. Its exact source is attached as \`ffmpeg-${FFMPEG.version}.tar.xz\` (from ${FFMPEG.source}, SHA-256 \`${FFMPEG.sha256}\`)`));
  }
  // The real changelog has the section for the first release.
  assert.ok(changelogSection(readFileSync(join(root, "..", "CHANGELOG.md"), "utf8"), "0.1.2").length > 0);
});

test("a platform release has its own changelog heading and only its own files", () => {
  const cl = "## [Chrome 0.3.0] - 2027-02-01\n\n- ext\n\n## [macOS 0.2.1] - 2027-01-02\n\n- mac\n\n## [Windows 0.2.1] - 2027-01-02\n\n- win\n";
  assert.equal(headingOf("0.2.1", "mac"), "macOS 0.2.1");
  assert.equal(headingOf("0.2.1", "all"), "0.2.1");
  const mac = notes(cl, "0.2.1", "mac", "ad-hoc");
  assert.match(mac, /^- mac\n/);
  assert.match(mac, /macos-arm64\.dmg/);
  assert.match(mac, /ffmpeg-/);
  assert.doesNotMatch(mac, /Windows|Chrome extension/);
  const win = notes(cl, "0.2.1", "windows");
  assert.match(win, /^- win\n/);
  assert.match(win, /setup-beta\.exe/);
  assert.match(win, /ffmpeg-/);
  assert.doesNotMatch(win, /macOS|\.dmg|Chrome extension/);
  const ext = notes(cl, "0.3.0", "chrome");
  assert.match(ext, /^- ext\n/);
  assert.match(ext, /extension-v0\.3\.0\.zip/);
  assert.doesNotMatch(ext, /\.dmg|\.exe|ffmpeg/);
});

test("the macOS signing is read from both build jobs' files, and must agree", () => {
  const dir = mkdtempSync(join(tmpdir(), "ksdm-notes-"));
  try {
    mkdirSync(join(dir, "mac-arm64")); mkdirSync(join(dir, "mac-x64"));
    writeFileSync(join(dir, "mac-arm64", "signing-macos-arm64.txt"), "ad-hoc\n");
    assert.throws(() => macSigning(dir), /expected signing-macos-arm64\.txt and signing-macos-x64\.txt/);
    writeFileSync(join(dir, "mac-x64", "signing-macos-x64.txt"), "developer-id\n");
    assert.throws(() => macSigning(dir), /signed differently/);
    writeFileSync(join(dir, "mac-x64", "signing-macos-x64.txt"), "ad-hoc\n");
    assert.equal(macSigning(dir), "ad-hoc");
  } finally {
    rmSync(dir, { recursive: true });
  }
});
