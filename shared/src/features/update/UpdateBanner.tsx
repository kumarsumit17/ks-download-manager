import { Download, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { formatBytes } from "@/features/downloads/format";
import { t } from "@/lib/i18n";
import { dismiss, installNow, noteText, useUpdates } from "./appUpdate";

/** A bar above the download list while a newer version is available (specs/ui.md, "App updates"). */
export function UpdateBanner() {
  const u = useUpdates();
  if (!u.available || (u.dismissed === u.available.version && (u.phase === "idle" || u.phase === "failed"))) return null;
  const busy = u.phase === "downloading" || u.phase === "checking";
  return (
    <div role="status" className="flex flex-wrap items-center gap-2 border-b bg-primary/10 px-3 py-2 text-sm">
      <span className="flex-1">{u.phase === "idle" || u.phase === "checking" ? t("common.appUpdate.available", { version: u.available.version }) : noteText(u.note)}</span>
      {u.phase !== "opened" && (
        <>
          <Button type="button" size="sm" disabled={busy} onClick={() => void installNow()}>
            {busy ? <Loader2 className="animate-spin motion-reduce:animate-none" aria-hidden /> : <Download aria-hidden />}
            {t("common.appUpdate.installButton", { size: formatBytes(u.available.size ?? null) })}
          </Button>
          <Button type="button" size="sm" variant="ghost" disabled={busy} onClick={dismiss}>
            {t("common.appUpdate.later")}
          </Button>
        </>
      )}
    </div>
  );
}
