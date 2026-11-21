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
  code?: string;
  error?: string;
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
