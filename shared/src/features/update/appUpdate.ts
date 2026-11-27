// App updates (specs/ui.md, "App updates"): one shared state for the banner in the main
// window and the App updates section in Settings. Rust does the checking, downloading and
// opening (src-tauri/src/appupdate.rs); the state is ks-utility's `createUpdateStore`
// (ks-utility specs/app-update.md), whose notes are text keys shown with `t`.
import { useSyncExternalStore } from "react";
import { createUpdateStore, type UpdateNote, type UpdateState } from "ks-utility";
import { api, onUpdateAvailable } from "@/features/downloads/api";
import { isMsg, t, type TextKey } from "@/lib/i18n";

export type { UpdateState } from "ks-utility";

/** When the app quits after the installer opened (ks-utility's `app_update::install`):
 * at once on Windows, after a moment on macOS so Finder can replace it. */
let quitAfterMs: number | null = null;

const store = createUpdateStore(
  {
    check: () => api.checkForUpdate(),
    install: async () => {
      const opened = await api.installUpdate();
      quitAfterMs = opened.quitAfterMs;
      return opened.quit ? "windows" : "macos";
    },
    // Rust's errors are message codes already.
    errorNote: (e) => (isMsg(e) ? { key: e.code, params: e.params } : { key: "common.error.generic" }),
  },
  "KS Download Manager",
);

let started = false;

/** Reads the version and any update already found, and listens for the automatic check. */
export function startUpdates() {
  if (started) return;
  started = true;
  void api.appVersion().then((v) => store.setCurrent(v));
  void api.updateStatus().then((available) => available && store.setAvailable(available));
  void onUpdateAvailable((available) => store.setAvailable(available));
}

/** "Check for updates". */
export const checkNow = () => store.checkNow();

/** Downloads the installer, checks it and opens it; the app then saves and closes (at once
 * on Windows, after a moment on macOS, while the note says so). */
export async function installNow() {
  if ((await store.installNow()) === null || quitAfterMs === null) return;
  const wait = quitAfterMs;
  setTimeout(() => void api.quit(), wait);
}

export const dismiss = () => store.dismiss();

/** A note from the store in the interface language. */
export function noteText(note: UpdateNote | null): string | null {
  // This app closes after opening the disk image, so it says so (ks-utility's store still
  // gives "openedMac", the text for an app that stays open).
  if (note?.key === "common.appUpdate.openedMac") return t("common.appUpdate.openedMacCloses", note.params);
  return note && t(note.key as TextKey, note.params);
}

export function useUpdates(): UpdateState {
  return useSyncExternalStore(store.subscribe, store.getState);
}
