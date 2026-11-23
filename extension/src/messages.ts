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

/** Chrome flags this download: a warning, or a scan Chrome is still running or asking for.
 * Those stay in Chrome, because Chrome's safety check does not apply to a file the app
 * downloads. "safe", "accepted" (the user kept it), and verdicts that clear a file are not
 * flags; neither is a missing value. */
export function isDangerous(danger: unknown): boolean {
  if (typeof danger !== "string" || danger === "") return false;
  return !["safe", "accepted", "allowlistedByPolicy", "deepScannedSafe"].includes(danger);
}

/** What the browser would send with this download: sent to the app with the link. */
export interface BrowserHeaders {
  cookie?: string;
  referer?: string;
  userAgent?: string;
}

/** Longest cookie text the app accepts (src-tauri/src/handoff.rs, MAX_COOKIE). */
export const MAX_COOKIE = 16 * 1024;

const printable = (s: string) => /^[\x20-\x7e]*$/.test(s);

/** Builds the headers from the site's cookies, the referring page and the browser's
 * identification; anything that could not be sent as a header value is left out. */
export function browserHeaders(cookies: { name: string; value: string }[], referrer: unknown, userAgent: unknown): BrowserHeaders {
  const h: BrowserHeaders = {};
  const cookie = cookies.map((c) => `${c.name}=${c.value}`).join("; ");
  if (cookie && cookie.length <= MAX_COOKIE && printable(cookie)) h.cookie = cookie;
  if (isHttpUrl(referrer) && printable(referrer)) h.referer = referrer;
  if (typeof userAgent === "string" && userAgent.length <= 512 && printable(userAgent)) h.userAgent = userAgent;
  return h;
}
