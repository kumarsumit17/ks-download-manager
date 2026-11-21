import { ArrowDown } from "lucide-react";
import type { Download } from "@/features/downloads/types";
import { formatSpeedOrZero } from "@/features/downloads/format";
import { t } from "@/lib/i18n";

export function StatusBar({ downloads }: { downloads: Download[] }) {
  const active = downloads.filter((d) => d.status === "downloading");
  const finished = downloads.filter((d) => d.status === "finished").length;
  const speed = active.reduce((s, d) => s + d.speed, 0);
  return (
    <footer className="flex h-7 items-center gap-3 border-t bg-muted/40 px-3 text-xs text-muted-foreground">
      <span>{t("statusBar.active", { count: active.length })}</span>
      <span aria-hidden>·</span>
      <span className="flex items-center gap-1 tabular-nums">
        <ArrowDown className="size-3.5" strokeWidth={1.75} aria-hidden />
        {formatSpeedOrZero(speed)}
      </span>
      <span aria-hidden>·</span>
      <span>{t("statusBar.finished", { count: finished })}</span>
    </footer>
  );
}
