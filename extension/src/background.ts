// Background service worker: the only part that talks to the app (native messaging). It
// accepts requests only from this extension's own content scripts and popup; web pages
// cannot message it (the manifest has no `externally_connectable`).

import { HOST, type Reply, type ToBackground, isHttpUrl, isSafe } from "./messages";
import { YOUTUBE_LINK_PATTERNS, youtubeVideoUrl } from "./site-rules";

async function toApp(message: object): Promise<Reply> {
  try {
    const r = (await chrome.runtime.sendNativeMessage(HOST, message)) as Reply | undefined;
    return r && typeof r.ok === "boolean" ? r : { ok: false, code: "unknown" };
  } catch {
    // "Specified native messaging host not found", or the host exited.
    return { ok: false, code: "app_not_installed" };
  }
}

function send(kind: "video" | "image" | "file", url: string): Promise<Reply> {
  return toApp({ type: "add", v: 1, kind, url });
}

// ---- Requests from our own content scripts and popup -----------------------------------

chrome.runtime.onMessage.addListener((raw, sender, respond) => {
  if (sender.id !== chrome.runtime.id) return;
  const m = raw as ToBackground;
  const work = (async (): Promise<unknown> => {
    switch (m?.type) {
      case "send":
        if ((m.kind !== "video" && m.kind !== "image") || !isHttpUrl(m.url)) return { ok: false, code: "bad_url" };
        return send(m.kind, m.url);
      case "ping":
        return toApp({ type: "ping", v: 1 });
      case "setDownloads":
        await chrome.storage.local.set({ handOffDownloads: m.on === true });
        return { ok: true };
      case "setSite":
        return setSite(m.origin, m.on === true, m.tabId);
      default:
        return { ok: false, code: "bad_request" };
    }
  })();
  work.then(respond, () => respond({ ok: false, code: "unknown" }));
  return true;
});

// ---- Per-site image button (opt-in) ----------------------------------------------------

const siteScriptId = (origin: string) => `hover:${origin}`;

/** Registers or removes the hover script for one origin. The permission itself is asked
 * for in the popup (it needs the user's click); this only follows it. */
async function setSite(origin: string, on: boolean, tabId?: number): Promise<Reply> {
  let o: URL;
  try {
    o = new URL(origin);
  } catch {
    return { ok: false, code: "bad_url" };
  }
  if (o.protocol !== "https:" && o.protocol !== "http:") return { ok: false, code: "bad_url" };
  const pattern = `${o.origin}/*`;
  const id = siteScriptId(o.origin);
  await chrome.scripting.unregisterContentScripts({ ids: [id] }).catch(() => {});
  if (!on) return { ok: true };
  if (!(await chrome.permissions.contains({ origins: [pattern] }))) return { ok: false, code: "no_permission" };
  await chrome.scripting.registerContentScripts([
    { id, matches: [pattern], js: ["hover.js"], runAt: "document_idle", persistAcrossSessions: true },
  ]);
  if (tabId !== undefined) await chrome.scripting.executeScript({ target: { tabId }, files: ["hover.js"] }).catch(() => {});
  return { ok: true };
}

// A site permission removed in Chrome's own settings also removes its script.
chrome.permissions.onRemoved.addListener((p) => {
  for (const pattern of p.origins ?? []) {
    const origin = pattern.replace(/\/\*$/, "");
    void chrome.scripting.unregisterContentScripts({ ids: [siteScriptId(origin)] }).catch(() => {});
  }
});

// ---- "Download video" on YouTube links anywhere ----------------------------------------

function menus() {
  void chrome.contextMenus.removeAll().then(() =>
    chrome.contextMenus.create({
      id: "video-link",
      title: chrome.i18n.getMessage("menuVideoLink"),
      contexts: ["link"],
      targetUrlPatterns: YOUTUBE_LINK_PATTERNS,
    }),
  );
}
chrome.runtime.onInstalled.addListener(menus);
chrome.runtime.onStartup.addListener(menus);
chrome.contextMenus.onClicked.addListener((info) => {
  if (info.menuItemId !== "video-link" || !info.linkUrl) return;
  const url = youtubeVideoUrl(info.linkUrl);
  if (url) void send("video", url);
});

// ---- Optional: hand ordinary browser downloads to the app ------------------------------
// Off by default. Needs the optional "downloads" permission, asked for when the user turns
// it on. Chrome's download is paused while the app checks the link; if the app takes it,
// Chrome's copy is cancelled and removed from Chrome's list, otherwise Chrome carries on.
// The app gets only the URL (no cookies), so downloads that need a sign-in stay in Chrome.
// A download Chrome flags as dangerous, or is still checking, is never taken over: Chrome's
// safety check does not apply to a file the app downloads.

const handled = new Set<number>();
let watching = false;

function watchDownloads() {
  if (watching || !chrome.downloads?.onCreated) return;
  watching = true;
  chrome.downloads.onCreated.addListener((item) => void offer(item));
}

async function offer(item: chrome.downloads.DownloadItem) {
  if (handled.has(item.id) || item.state !== "in_progress" || item.byExtensionId || item.incognito) return;
  if (!isSafe(item.danger)) return;
  const { handOffDownloads } = await chrome.storage.local.get("handOffDownloads");
  if (handOffDownloads !== true) return;
  const url = item.finalUrl || item.url;
  if (!isHttpUrl(url)) return;
  handled.add(item.id);
  await chrome.downloads.pause(item.id).catch(() => {});
  // Chrome may have flagged it in the meantime: look again before handing it over.
  const [now] = await chrome.downloads.search({ id: item.id }).catch(() => []);
  if (!now || !isSafe(now.danger)) {
    await chrome.downloads.resume(item.id).catch(() => {});
    return;
  }
  const r = await send("file", url);
  if (r.ok) {
    await chrome.downloads.cancel(item.id).catch(() => {});
    await chrome.downloads.erase({ id: item.id }).catch(() => {});
  } else {
    await chrome.downloads.resume(item.id).catch(() => {});
  }
}

watchDownloads();
// The permission may be granted while the worker runs.
chrome.permissions.onAdded.addListener((p) => {
  if (p.permissions?.includes("downloads")) watchDownloads();
});
