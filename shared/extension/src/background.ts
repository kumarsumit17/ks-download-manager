// Background service worker: the only part that talks to the app (native messaging). It
// accepts requests only from this extension's own content scripts and popup; web pages
// cannot message it (the manifest has no `externally_connectable`).

import { HOST, type Reply, type ToBackground, browserHeaders, isDangerous, isHttpUrl } from "./messages";
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

// ---- Hand browser downloads to the app -----------------------------------------------
// On by default; the popup's switch turns it off (`handOffDownloads: false`). Every download
// Chrome starts is paused while the app checks the link; if the app takes it, Chrome's copy
// is cancelled and removed from Chrome's list, otherwise Chrome carries on. With the link go
// the site's cookies for it, the page it came from and Chrome's identification, so a site
// that serves the file only to this browser session serves it to the app too; the app uses
// them for that download only and never saves them. A download Chrome flags as dangerous
// stays in Chrome: Chrome's safety check does not apply to a file the app downloads.
// The last outcome is kept (`lastHandOff`) so the popup can say what happened.

const handled = new Set<number>();

async function handOffOn(): Promise<boolean> {
  const { handOffDownloads } = await chrome.storage.local.get("handOffDownloads");
  return handOffDownloads !== false;
}

function note(code: string, url: string) {
  let host = "";
  try {
    host = new URL(url).hostname;
  } catch {
    host = "";
  }
  void chrome.storage.local.set({ lastHandOff: { at: Date.now(), code, host } });
}

async function offer(item: chrome.downloads.DownloadItem) {
  const url = item.finalUrl || item.url;
  // Chrome's own pages, files made by a page (blob:, data:) and private windows are left alone.
  if (handled.has(item.id) || item.state !== "in_progress" || item.incognito || !isHttpUrl(url)) return;
  // A download another extension started is that extension's.
  if (item.byExtensionId) return;
  if (!(await handOffOn())) return;
  if (isDangerous(item.danger)) return note("chrome_warned", url);
  handled.add(item.id);
  await chrome.downloads.pause(item.id).catch(() => {});
  // Chrome may have flagged it in the meantime: look again before handing it over.
  const [now] = await chrome.downloads.search({ id: item.id }).catch(() => []);
  if (!now || now.state !== "in_progress" || isDangerous(now.danger)) {
    if (now?.state === "in_progress") await chrome.downloads.resume(item.id).catch(() => {});
    return note("chrome_warned", url);
  }
  const cookies = await chrome.cookies.getAll({ url }).catch(() => []);
  const headers = browserHeaders(cookies, now.referrer, navigator.userAgent);
  const r = await toApp({ type: "add", v: 1, kind: "file", url, headers });
  if (r.ok) {
    await chrome.downloads.cancel(item.id).catch(() => {});
    await chrome.downloads.erase({ id: item.id }).catch(() => {});
    note("sent", url);
  } else {
    await chrome.downloads.resume(item.id).catch(() => {});
    note(r.code ?? "unknown", url);
  }
}

// Registered when the worker starts, so Chrome wakes the worker for every new download.
chrome.downloads.onCreated.addListener((item) => void offer(item));
