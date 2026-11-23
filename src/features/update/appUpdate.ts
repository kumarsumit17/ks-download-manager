// App updates (specs/ui.md, "App updates"): one shared state for the banner in the main
// window and the App updates section in Settings. Rust does the checking, downloading and
// opening (src-tauri/src/appupdate.rs); this only keeps what to show.
import { useSyncExternalStore } from "react";
import { api, message, onUpdateAvailable, type AppUpdate } from "@/features/downloads/api";
import { t } from "@/lib/i18n";

/** "failed": the last install attempt failed; the note says why, and it can be tried again. */
export type Phase = "idle" | "checking" | "downloading" | "opened" | "failed";

export interface UpdateState {
  /** The running version; "0.0.0" when built from source. */
  current: string | null;
  available: AppUpdate | null;
  phase: Phase;
  /** The last result or error, as text for the user. */
  note: string | null;
  /** The banner was closed with "Later" for this version. */
  dismissed: string | null;
}

let state: UpdateState = { current: null, available: null, phase: "idle", note: null, dismissed: null };
const listeners = new Set<() => void>();

function set(patch: Partial<UpdateState>) {
  state = { ...state, ...patch };
  for (const f of listeners) f();
}

let started = false;

/** Reads the version and any update already found, and listens for the start-up check. */
export function startUpdates() {
  if (started) return;
  started = true;
  void api.appVersion().then((current) => set({ current }));
  void api.updateStatus().then((available) => available && set({ available }));
  void onUpdateAvailable((available) => set({ available }));
}

/** "Check for updates". */
export async function checkNow() {
  set({ phase: "checking", note: t("appUpdate.checking") });
  try {
    const r = await api.checkForUpdate();
    if (r.result === "available") set({ phase: "idle", available: r.update, dismissed: null, note: t("appUpdate.available", { version: r.update.version }) });
    else set({ phase: "idle", available: null, note: t("appUpdate.upToDate", { version: r.version }) });
  } catch (e) {
    set({ phase: "idle", note: message(e) });
  }
}

/** Downloads the installer, checks it and opens it; on Windows the app then closes. */
export async function installNow() {
  const version = state.available?.version;
  if (!version) return;
  set({ phase: "downloading", note: t("appUpdate.downloading", { version }) });
  try {
    const r = await api.installUpdate();
    set({ phase: "opened", note: t(r.quit ? "appUpdate.openedWindows" : "appUpdate.openedMac") });
    if (r.quit) await api.quit();
  } catch (e) {
    set({ phase: "failed", note: message(e) });
  }
}

export function dismiss() {
  set({ dismissed: state.available?.version ?? null });
}

export function useUpdates(): UpdateState {
  return useSyncExternalStore(
    (f) => {
      listeners.add(f);
      return () => listeners.delete(f);
    },
    () => state,
  );
}
