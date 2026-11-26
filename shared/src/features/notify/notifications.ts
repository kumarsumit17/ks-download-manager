// System notifications when a download finishes or fails (per-platform plan, step 2): only
// while the window is not in front, and only when Settings allows it (src-tauri's `notify`
// checks that). The text is in the user's language; Rust only shows it.
import { api, onChanged } from "@/features/downloads/api";
import type { DownloadRecord, Status } from "@/features/downloads/types";
import { t } from "@/lib/i18n";

const last = new Map<number, Status>();
let started = false;

export function startNotifications() {
  if (started) return;
  started = true;
  void api.list().then((records) => records.forEach((r) => last.set(r.id, r.status)));
  void onChanged((r: DownloadRecord) => {
    const before = last.get(r.id);
    last.set(r.id, r.status);
    // Only a change into finished or failed, never a record seen that way already.
    if (before === undefined || before === r.status || document.hasFocus()) return;
    if (r.status === "finished") void api.notify(t("notify.finished"), r.name).catch(() => {});
    else if (r.status === "failed") void api.notify(t("notify.failed"), r.name).catch(() => {});
  });
}
