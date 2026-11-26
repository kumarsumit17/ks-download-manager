import { test } from "node:test";
import assert from "node:assert/strict";
import { linkFromShortcut, linkFromText } from "../../src/features/downloads/dropLink.ts";

test("a dragged link is taken from a uri-list or plain text", () => {
  assert.equal(linkFromText("# from Chrome\r\nhttps://a.example/f.zip\r\nhttps://b.example/"), "https://a.example/f.zip");
  assert.equal(linkFromText("  http://a.example/x  "), "http://a.example/x");
  for (const not of ["", "just words", "ftp://a.example/f", "javascript:alert(1)", "https://user:pw@a.example/", "file:///etc/passwd"]) {
    assert.equal(linkFromText(not), null, not);
  }
});

test("a .webloc or .url file gives its link", () => {
  const webloc = `<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>URL</key><string>https://a.example/v?a=1&amp;b=2</string></dict></plist>`;
  assert.equal(linkFromShortcut("Video.webloc", webloc), "https://a.example/v?a=1&b=2");
  assert.equal(linkFromShortcut("File.URL", "[InternetShortcut]\r\nURL=https://a.example/f.zip\r\n"), "https://a.example/f.zip");
  assert.equal(linkFromShortcut("Bad.url", "[InternetShortcut]\r\nURL=file:///C:/Windows\r\n"), null);
  assert.equal(linkFromShortcut("notes.txt", "https://a.example/"), null, "only shortcut files");
});
