import { test } from "node:test";
import assert from "node:assert/strict";
import { xLargestImage, youtubeVideoUrl } from "../src/site-rules.ts";

test("YouTube watch, Shorts and youtu.be links become one canonical video address", () => {
  const want = "https://www.youtube.com/watch?v=jNQXAC9IVRw";
  for (const href of [
    "https://www.youtube.com/watch?v=jNQXAC9IVRw",
    "https://www.youtube.com/watch?v=jNQXAC9IVRw&list=PL123&index=2&t=30s",
    "https://m.youtube.com/watch?v=jNQXAC9IVRw",
    "https://youtube.com/watch?feature=share&v=jNQXAC9IVRw",
    "https://www.youtube.com/shorts/jNQXAC9IVRw",
    "https://youtu.be/jNQXAC9IVRw?si=abc",
  ]) {
    assert.equal(youtubeVideoUrl(href), want, href);
  }
  for (const href of [
    "https://www.youtube.com/",
    "https://www.youtube.com/playlist?list=PL123",
    "https://www.youtube.com/watch?v=short",
    "https://www.youtube.com/@channel",
    "https://evil.example/watch?v=jNQXAC9IVRw",
    "https://youtube.com.evil.example/watch?v=jNQXAC9IVRw",
    "javascript:alert(1)",
    "not a url",
  ]) {
    assert.equal(youtubeVideoUrl(href), null, href);
  }
});

test("X images are asked for at their original size", () => {
  assert.equal(
    xLargestImage("https://pbs.twimg.com/media/GabCdEfXYZ?format=jpg&name=small"),
    "https://pbs.twimg.com/media/GabCdEfXYZ?format=jpg&name=orig",
  );
  assert.equal(
    xLargestImage("https://pbs.twimg.com/media/GabCdEfXYZ?format=webp&name=900x900"),
    "https://pbs.twimg.com/media/GabCdEfXYZ?format=webp&name=orig",
  );
  assert.equal(
    xLargestImage("https://pbs.twimg.com/media/GabCdEfXYZ.png:large"),
    "https://pbs.twimg.com/media/GabCdEfXYZ?format=png&name=orig",
  );
  assert.equal(
    xLargestImage("https://pbs.twimg.com/ext_tw_video_thumb/1/pu/img/Abc.jpg"),
    "https://pbs.twimg.com/ext_tw_video_thumb/1/pu/img/Abc?format=jpg&name=orig",
  );
  assert.equal(
    xLargestImage("https://pbs.twimg.com/profile_images/1/Ab_c_normal.jpg"),
    "https://pbs.twimg.com/profile_images/1/Ab_c.jpg",
  );
  assert.equal(
    xLargestImage("https://pbs.twimg.com/profile_images/1/Abc_400x400.png"),
    "https://pbs.twimg.com/profile_images/1/Abc.png",
  );
  // Other hosts and unknown paths are left alone.
  for (const s of ["https://abs.twimg.com/emoji/v2/svg/1f600.svg", "https://example.com/media/a.jpg?name=small", "x"]) {
    assert.equal(xLargestImage(s), s);
  }
});
