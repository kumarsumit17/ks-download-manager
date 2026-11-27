// App updates (specs/ui.md, "App updates"): one shared state for the banner in the main
// window and the App updates section in Settings. Rust does the checking, downloading and
// opening (src-tauri/src/appupdate.rs); the state is ks-utility's `createUpdateStore`
// (ks-utility specs/app-update.md), whose notes are text keys shown with `t`.
import { useSyncExternalStore } from "react";
import { createUpdateStore, MAC_QUIT_AFTER_MS, type UpdateNote, type UpdateState } from "ks-utility";
import { api, onUpdateAvailable } from "@/features/downloads/api";
import { isMsg, t, type TextKey } from "@/lib/i18n";

export type { UpdateState } from "ks-utility";

const store = createUpdateStore(
  {
    check: () => api.checkForUpdate(),
    // Rust opens it with ks-utility's `app_update::install::open`: the app then quits.
    install: async () => ((await api.installUpdate()).quit ? "windows" : "macos-closes"),
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
  const opened = await store.installNow();
  if (opened === "windows") await api.quit();
  else if (opened === "macos-closes") setTimeout(() => void api.quit(), MAC_QUIT_AFTER_MS);
}

export const dismiss = () => store.dismiss();

/** A note from the store in the interface language. */
export function noteText(note: UpdateNote | null): string | null {
  return note && t(note.key as TextKey, note.params);
}

export function useUpdates(): UpdateState {
  return useSyncExternalStore(store.subscribe, store.getState);
}
