// Messages between the extension's parts, and to the app's native host
// (src-tauri/src/handoff.rs, `Request` and `Reply`). Only a kind and an http(s) URL ever
// leave the browser: never cookies, page content or the page address.

export type Kind = "video" | "image" | "file";

/** Content script or popup → background. */
export type ToBackground =
  | { type: "send"; kind: "video" | "image"; url: string }
  | { type: "ping" }
  | { type: "setDownloads"; on: boolean }
  | { type: "setSite"; origin: string; on: boolean; tabId?: number };

/** The app's answer, plus an `app_not_installed` code when Chrome cannot find the host. */
export interface Reply {
  ok: boolean;
  id?: number;
  /** A message code, shown through `_locales` in the browser's language (`err_<code>`). */
  code?: string;
  /** On a ping: a newer app version the app's own update check found. */
  update?: string;
}

/** Where a new version is downloaded; the extension is loaded unpacked, so it cannot update itself. */
export const RELEASES = "https://github.com/kumarsumit17/ks-download-manager/releases/latest";

/** `MAJOR.MINOR.PATCH` compared by number. A copy built from source ("0.0.0") never asks. */
export function isNewer(candidate: unknown, current: string): candidate is string {
  if (typeof candidate !== "string" || !/^\d+(\.\d+){0,3}$/.test(candidate) || current === "0.0.0") return false;
  const a = candidate.split(".").map(Number);
  const b = current.split(".").map(Number);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const d = (a[i] ?? 0) - (b[i] ?? 0);
    if (d !== 0) return d > 0;
  }
  return false;
}

export const HOST = "com.ks.downloadmanager";

export function isHttpUrl(s: unknown): s is string {
  if (typeof s !== "string" || s.length > 2048) return false;
  try {
    const u = new URL(s);
    return (u.protocol === "https:" || u.protocol === "http:") && u.username === "" && u.password === "";
  } catch {
    return false;
  }
}

/** The extension's own label for a reply code (from `_locales`). */
export function replyText(r: Reply): string {
  if (r.ok) return chrome.i18n.getMessage("sent");
  const key = `err_${r.code ?? "unknown"}`;
  return chrome.i18n.getMessage(key) || chrome.i18n.getMessage("err_unknown");
}

/** Chrome's verdict allows a hand-off: "safe", or "accepted" (the user chose to keep it).
 * Every warning, and every scan still running, keeps the download in Chrome. */
export function isSafe(danger: unknown): boolean {
  return danger === "safe" || danger === "accepted";
}
