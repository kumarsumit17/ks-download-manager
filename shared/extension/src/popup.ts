// The toolbar popup: whether the app answers, a newer version if the app found one, the
// image button for the current site (opt-in per site), the hand-off of browser downloads
// (on unless switched off), and what happened to the last browser download.
import { isNewer, RELEASES, type Reply } from "./messages";
import { X_HOSTS } from "./site-rules";

const t = (key: string, sub?: string | string[]) => chrome.i18n.getMessage(key, sub);

for (const el of document.querySelectorAll<HTMLElement>("[data-i18n]")) el.textContent = t(el.dataset.i18n ?? "");

const status = document.getElementById("status") as HTMLParagraphElement;
chrome.runtime.sendMessage<Reply>({ type: "ping" }).then((r) => {
  const ok = r.ok || r.code === "app_not_running";
  status.dataset.ok = String(ok);
  status.textContent = r.ok ? t("statusConnected") : r.code === "app_not_running" ? t("statusNotRunning") : t("statusNotInstalled");
  // The app's own update check found a newer version (the extension never checks the network).
  if (isNewer(r.update, chrome.runtime.getManifest().version)) {
    (document.getElementById("update-text") as HTMLParagraphElement).textContent = t("updateAvailable", r.update);
    (document.getElementById("update") as HTMLElement).hidden = false;
  }
});

document.getElementById("update-button")?.addEventListener("click", () => {
  void chrome.tabs.create({ url: RELEASES });
});

async function siteSection() {
  const box = document.getElementById("site") as HTMLElement;
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  let origin: URL | null = null;
  try {
    origin = tab?.url ? new URL(tab.url) : null;
  } catch {
    origin = null;
  }
  if (!origin || (origin.protocol !== "https:" && origin.protocol !== "http:")) {
    box.textContent = t("siteNotAvailable");
    return;
  }
  const host = origin.hostname.replace(/^www\./, "");
  if (host === "youtube.com" || X_HOSTS.includes(host)) {
    box.textContent = t("siteBuiltIn");
    return;
  }
  const label = document.createElement("label");
  label.className = "row";
  const box2 = document.createElement("input");
  box2.type = "checkbox";
  const registered = await chrome.scripting.getRegisteredContentScripts({ ids: [`hover:${origin.origin}`] }).catch(() => []);
  box2.checked = registered.length > 0;
  const span = document.createElement("span");
  span.textContent = t("siteToggle", origin.hostname);
  label.append(box2, span);
  box.append(label);
  box2.addEventListener("change", async () => {
    const r = await chrome.runtime.sendMessage<Reply>({ type: "setSite", origin: origin.origin, on: box2.checked, tabId: tab.id });
    if (!r.ok) box2.checked = !box2.checked;
  });
}
void siteSection();

async function downloadsSection() {
  const box = document.getElementById("downloads") as HTMLInputElement;
  const { handOffDownloads, lastHandOff } = await chrome.storage.local.get(["handOffDownloads", "lastHandOff"]);
  box.checked = handOffDownloads !== false;
  // Saved by the background worker, so it is kept even if the popup closes at once.
  box.addEventListener("change", () => void chrome.runtime.sendMessage({ type: "setDownloads", on: box.checked }));
  const last = document.getElementById("last") as HTMLParagraphElement;
  last.textContent = lastText(lastHandOff);
}
void downloadsSection();

/** "Last download (site): sent to the app", or why it stayed in Chrome. */
function lastText(v: unknown): string {
  const l = v as { code?: unknown; host?: unknown } | undefined;
  if (!l || typeof l.code !== "string") return "";
  const host = typeof l.host === "string" ? l.host : "";
  if (l.code === "sent") return t("lastSent", host);
  return t("lastKept", [host, t(`err_${l.code}`) || t("err_unknown")]);
}
