// The menu bar (macOS; desktop/mac builds it, src-tauri's `set_app_menu`). This file only
// gives it its labels in the user's language, by item id, and is called again whenever the
// language changes. On Windows the call does nothing.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { onLanguage, t } from "@/lib/i18n";

function labels(): Record<string, string> {
  return {
    about: t("menu.about"),
    services: t("menu.services"),
    hide: t("menu.hide"),
    hideOthers: t("menu.hideOthers"),
    showAll: t("menu.showAll"),
    quit: t("menu.quit"),
    settings: `${t("common.settings")}…`,
    checkUpdates: `${t("common.appUpdate.checkButton")}…`,
    "menu.file": t("details.file"),
    add: `${t("add.title")}…`,
    open: t("common.open"),
    reveal: t("menu.reveal"),
    delete: `${t("common.delete")}…`,
    closeWindow: t("menu.closeWindow"),
    "menu.edit": t("menu.edit"),
    undo: t("menu.undo"),
    redo: t("menu.redo"),
    cut: t("menu.cut"),
    copy: t("common.copy"),
    paste: t("common.paste"),
    selectAll: t("playlist.selectAll"),
    "menu.downloads": t("menu.downloads"),
    resume: t("common.resume"),
    pause: t("common.pause"),
    queues: `${t("queues")}…`,
    clearAll: `${t("clear.confirm")}…`,
    "menu.view": t("menu.view"),
    "view.all": t("sidebar.all"),
    "view.unfinished": t("sidebar.unfinished"),
    "view.finished": t("sidebar.finished"),
    "view.video": t("category.video"),
    "view.audio": t("category.audio"),
    "view.images": t("category.images"),
    "view.other": t("category.other"),
    find: t("common.search"),
    "menu.window": t("menu.window"),
    minimize: t("menu.minimize"),
    zoom: t("menu.zoom"),
    fullscreen: t("menu.fullscreen"),
    "menu.help": t("menu.help"),
    help: t("menu.helpItem"),
  };
}

let started = false;

/** Sets the menu bar now and after every language change. */
export function startAppMenu() {
  if (started) return;
  started = true;
  const set = () => void invoke("set_app_menu", { labels: labels() }).catch((e) => console.error("menu bar", e));
  set();
  onLanguage(set);
}

/** A menu item was chosen: its id (`add`, `view.video`, …). */
export function onMenu(f: (id: string) => void): Promise<UnlistenFn> {
  return listen<string>("menu", (e) => f(e.payload));
}
