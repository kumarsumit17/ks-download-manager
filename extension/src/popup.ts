// The toolbar popup: whether the app answers, the image button for the current site
// (opt-in, through an optional host permission for that one site), and the optional
// hand-off of browser downloads (through the optional "downloads" permission).
import type { Reply } from "./messages";
import { X_HOSTS } from "./site-rules";

const t = (key: string, sub?: string) => chrome.i18n.getMessage(key, sub);

for (const el of document.querySelectorAll<HTMLElement>("[data-i18n]")) el.textContent = t(el.dataset.i18n ?? "");

const status = document.getElementById("status") as HTMLParagraphElement;
chrome.runtime.sendMessage<Reply>({ type: "ping" }).then((r) => {
  const ok = r.ok || r.code === "app_not_running";
  status.dataset.ok = String(ok);
  status.textContent = r.ok ? t("statusConnected") : r.code === "app_not_running" ? t("statusNotRunning") : t("statusNotInstalled");
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
  const pattern = `${origin.origin}/*`;
  const label = document.createElement("label");
  label.className = "row";
  const box2 = document.createElement("input");
  box2.type = "checkbox";
  box2.checked = await chrome.permissions.contains({ origins: [pattern] });
  const span = document.createElement("span");
  span.textContent = t("siteToggle", origin.hostname);
  label.append(box2, span);
  box.append(label);
  box2.addEventListener("change", async () => {
    if (box2.checked) {
      // Must run straight from the click: Chrome asks the user for this one site.
      const granted = await chrome.permissions.request({ origins: [pattern] });
      if (!granted) {
        box2.checked = false;
        return;
      }
      await chrome.runtime.sendMessage({ type: "setSite", origin: origin.origin, on: true, tabId: tab.id });
    } else {
      await chrome.runtime.sendMessage({ type: "setSite", origin: origin.origin, on: false });
      await chrome.permissions.remove({ origins: [pattern] });
    }
  });
}
void siteSection();

async function downloadsSection() {
  const box = document.getElementById("downloads") as HTMLInputElement;
  const { handOffDownloads } = await chrome.storage.local.get("handOffDownloads");
  box.checked = handOffDownloads === true && (await chrome.permissions.contains({ permissions: ["downloads"] }));
  box.addEventListener("change", async () => {
    if (box.checked) {
      const granted = await chrome.permissions.request({ permissions: ["downloads"] });
      if (!granted) {
        box.checked = false;
        return;
      }
    }
    await chrome.runtime.sendMessage({ type: "setDownloads", on: box.checked });
    if (!box.checked) await chrome.permissions.remove({ permissions: ["downloads"] });
  });
}
void downloadsSection();
