import { test } from "node:test";
import assert from "node:assert/strict";
import { largestImage, parseSrcset, pickLargest } from "../src/largest-image.ts";

const base = "https://example.com/gallery/page.html";

test("srcset candidates are parsed per the HTML syntax", () => {
  assert.deepEqual(parseSrcset("a.jpg 480w, b.jpg 1080w", base), [
    { url: "https://example.com/gallery/a.jpg", width: 480 },
    { url: "https://example.com/gallery/b.jpg", width: 1080 },
  ]);
  assert.deepEqual(parseSrcset("/x.png, /x@2x.png 2x", base), [
    { url: "https://example.com/x.png", density: 1 },
    { url: "https://example.com/x@2x.png", density: 2 },
  ]);
  // Commas inside a URL (image CDNs) stay part of it.
  assert.deepEqual(parseSrcset("https://cdn.example/w_400,h_300/p.jpg 400w", base), [
    { url: "https://cdn.example/w_400,h_300/p.jpg", width: 400 },
  ]);
  // Data URLs and junk are skipped.
  assert.deepEqual(parseSrcset("data:image/png;base64,AAAA 1x, javascript:x 2x, 3x", base).length, 1);
});

test("the largest width wins, else the largest density", () => {
  assert.equal(pickLargest(parseSrcset("s.jpg 300w, l.jpg 1600w, m.jpg 800w", base)), "https://example.com/gallery/l.jpg");
  assert.equal(pickLargest(parseSrcset("a.jpg, b.jpg 3x, c.jpg 2x", base)), "https://example.com/gallery/b.jpg");
  assert.equal(pickLargest([]), null);
});

test("a linked image file beats srcset, srcset beats the shown image, data URLs are refused", () => {
  const facts = { base, currentSrc: "thumb.jpg", srcset: "m.jpg 800w, l.jpg 1600w", sources: [], linkHref: null as string | null };
  assert.equal(largestImage({ ...facts, linkHref: "https://example.com/full/photo.JPG" }), "https://example.com/full/photo.JPG");
  assert.equal(largestImage({ ...facts, linkHref: "https://example.com/post/42" }), "https://example.com/gallery/l.jpg");
  assert.equal(largestImage({ ...facts, srcset: null, sources: ["p.webp 2000w"] }), "https://example.com/gallery/p.webp");
  assert.equal(largestImage({ ...facts, srcset: null }), "https://example.com/gallery/thumb.jpg");
  assert.equal(largestImage({ ...facts, srcset: null, currentSrc: "data:image/png;base64,AA" }), null);
  assert.equal(largestImage({ ...facts, srcset: null, currentSrc: "blob:https://example.com/1" }), null);
});
