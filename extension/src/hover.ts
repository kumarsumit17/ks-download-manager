// Image button: hovering an image shows "Download", which sends the address of the largest
// version of that image. Built in on X; on other sites only after the user enables it for
// that site (popup), which registers this script for that one site.

import { pageButton } from "./button";
import { largestImage } from "./largest-image";
import type { Reply } from "./messages";
import { X_HOSTS, xLargestImage } from "./site-rules";

/** Images smaller than this on screen (icons, emoji, avatars in lists) get no button. */
const MIN_SIZE = 100;

// Injected again (enabled while the tab is open) must not add a second button.
const w = window as unknown as { __ksImageButton?: boolean };
if (!w.__ksImageButton) {
  w.__ksImageButton = true;
  start();
}

function start() {
  const onX = X_HOSTS.includes(location.hostname.replace(/^www\./, "").replace(/^mobile\./, ""));
  let current: HTMLImageElement | null = null;

  const btn = pageButton(() => {
    const img = current;
    if (!img) return;
    const url = addressFor(img, onX);
    if (!url) {
      btn.result({ ok: false, code: "no_image_url" });
      return;
    }
    chrome.runtime.sendMessage<Reply>({ type: "send", kind: "image", url }).then(
      (r) => btn.result(r),
      () => btn.result({ ok: false, code: "unknown" }),
    );
  });

  const place = (img: HTMLImageElement) => {
    const r = img.getBoundingClientRect();
    btn.host.style.left = `${Math.max(4, r.right - 120)}px`;
    btn.host.style.top = `${Math.max(4, r.top + 8)}px`;
  };

  // Sites often lay a transparent element over the image (X does), so the image is found
  // under the pointer rather than taken from the event target.
  document.addEventListener(
    "mousemove",
    (e) => {
      if (e.composedPath().includes(btn.host)) return;
      const img = imageAt(e.clientX, e.clientY);
      if (img !== current) {
        current = img;
        if (img) place(img);
        btn.show(img !== null);
      }
    },
    { passive: true },
  );
  window.addEventListener("scroll", () => current && place(current), { passive: true });
}

function imageAt(x: number, y: number): HTMLImageElement | null {
  for (const el of document.elementsFromPoint(x, y)) {
    if (el instanceof HTMLImageElement) {
      const r = el.getBoundingClientRect();
      return r.width >= MIN_SIZE && r.height >= MIN_SIZE && el.currentSrc ? el : null;
    }
  }
  return null;
}

function addressFor(img: HTMLImageElement, onX: boolean): string | null {
  const url = largestImage({
    base: document.baseURI,
    currentSrc: img.currentSrc || img.src,
    srcset: img.getAttribute("srcset"),
    sources:
      img.parentElement instanceof HTMLPictureElement
        ? [...img.parentElement.querySelectorAll("source")].map((s) => s.getAttribute("srcset") ?? "").filter(Boolean)
        : [],
    linkHref: img.closest("a")?.href ?? null,
  });
  if (!url) return null;
  return onX ? xLargestImage(url) : url;
}
