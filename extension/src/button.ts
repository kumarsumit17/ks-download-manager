// One "Download" button in a closed shadow root, so the page can neither style it nor
// reach it from its scripts. It reacts only to real clicks by the user (`isTrusted`), so a
// page cannot press it for them.

import { type Reply, replyText } from "./messages";

export interface PageButton {
  host: HTMLElement;
  button: HTMLButtonElement;
  show(on: boolean): void;
  /** Shows the outcome for a moment, then the label again. */
  result(r: Reply): void;
}

const STYLE = `
  :host { all: initial; position: fixed; z-index: 2147483647; }
  button {
    font: 600 13px/1 system-ui, -apple-system, "Segoe UI", sans-serif;
    color: #fff; background: #1d4ed8; border: 0; border-radius: 999px;
    padding: 8px 14px; cursor: pointer; box-shadow: 0 2px 8px rgb(0 0 0 / .35);
    display: inline-flex; align-items: center; gap: 6px;
  }
  button:hover { background: #1e40af; }
  button:focus-visible { outline: 3px solid #93c5fd; outline-offset: 2px; }
  button[data-state="busy"] { opacity: .8; cursor: progress; }
  button[data-state="error"] { background: #b91c1c; }
  @media (prefers-reduced-motion: no-preference) { button { transition: background .12s; } }
`;

const ARROW = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 4v12M6 11l6 6 6-6M5 20h14"/></svg>`;

export function pageButton(onClick: () => void): PageButton {
  const host = document.createElement("ks-download-button");
  const root = host.attachShadow({ mode: "closed" });
  const style = document.createElement("style");
  style.textContent = STYLE;
  const button = document.createElement("button");
  button.type = "button";
  const label = chrome.i18n.getMessage("download");
  const setLabel = (text: string) => {
    button.innerHTML = ARROW;
    button.append(document.createTextNode(text));
  };
  setLabel(label);
  button.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (!e.isTrusted || button.dataset.state === "busy") return;
    button.dataset.state = "busy";
    setLabel(chrome.i18n.getMessage("sending"));
    onClick();
  });
  // Clicks must not reach the page (X opens the image viewer on any click inside a post).
  for (const ev of ["mousedown", "mouseup", "pointerdown", "pointerup"]) host.addEventListener(ev, (e) => e.stopPropagation());
  root.append(style, button);
  host.style.display = "none";
  document.documentElement.append(host);
  let timer: ReturnType<typeof setTimeout> | undefined;
  return {
    host,
    button,
    show(on) {
      host.style.display = on ? "block" : "none";
    },
    result(r) {
      button.dataset.state = r.ok ? "done" : "error";
      setLabel(replyText(r));
      button.title = r.ok ? "" : (r.error ?? "");
      clearTimeout(timer);
      timer = setTimeout(() => {
        delete button.dataset.state;
        button.title = "";
        setLabel(label);
      }, r.ok ? 2000 : 5000);
    },
  };
}
