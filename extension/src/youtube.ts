// YouTube: one Download button on watch and Shorts pages. One click sends the video's
// address to the app, which downloads the best available quality. No quality menu.
//
// The button floats at the bottom right of the window instead of being placed inside
// YouTube's own toolbar: YouTube changes its page structure often, and a button that
// depends only on the address keeps working when it does. youtu.be links open a watch
// page, where the button appears too; links to videos elsewhere get a right-click menu
// item (background.ts).

import { pageButton } from "./button";
import type { Reply } from "./messages";
import { youtubeVideoUrl } from "./site-rules";

const btn = pageButton(() => {
  const url = youtubeVideoUrl(location.href);
  if (!url) return;
  chrome.runtime.sendMessage<Reply>({ type: "send", kind: "video", url }).then(
    (r) => btn.result(r),
    () => btn.result({ ok: false, code: "unknown" }),
  );
});
btn.host.style.right = "24px";
btn.host.style.bottom = "24px";

let last = "";
function update() {
  if (location.href === last) return;
  last = location.href;
  btn.show(youtubeVideoUrl(location.href) !== null);
}

// YouTube is a single-page app: the address changes without a page load.
document.addEventListener("yt-navigate-finish", update);
window.addEventListener("popstate", update);
setInterval(update, 1000);
update();
