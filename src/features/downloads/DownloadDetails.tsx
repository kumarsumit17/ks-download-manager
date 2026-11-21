import { useState } from "react";
import { Ban, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { api, message } from "./api";
import { msgText, t } from "@/lib/i18n";
import { formatBytes, formatSpeedOrZero, formatTimeLeft } from "./format";
import { cannotPauseText, type ConnectionTick, type Download, VIDEO_OUTDATED, cannotPause } from "./types";

/** Installs the newest video component, then retries the download. */
function UpdateVideoComponent({ d }: { d: Download }) {
  const [state, setState] = useState<{ busy: boolean; note: string | null }>({ busy: false, note: null });
  const run = async () => {
    setState({ busy: true, note: t("details.updatingVideo") });
    try {
      const r = await api.updateVideoComponent();
      if (r.result === "updated") {
        setState({ busy: false, note: t("details.updatedRetry", { from: r.from, to: r.to }) });
        await api.resume(d.id);
      } else {
        setState({ busy: false, note: t("details.newestTryLater", { version: r.version }) });
      }
    } catch (e) {
      setState({ busy: false, note: message(e) });
    }
  };
  return (
    <div className="flex items-center gap-3">
      <Button size="sm" variant="outline" disabled={state.busy} onClick={() => void run()}>
        <RefreshCw className={state.busy ? "animate-spin motion-reduce:animate-none" : ""} aria-hidden /> {t("updateVideo")}
      </Button>
      {state.note && <span role="status" className="text-xs text-muted-foreground">{state.note}</span>}
    </div>
  );
}

function pct(done: number, total: number): number {
  return total > 0 ? Math.min(100, (done / total) * 100) : 0;
}

/** One bar per connection: how far it is through its current range. */
function ConnectionRow({ index, c }: { index: number; c: ConnectionTick | null }) {
  if (!c) {
    return (
      <li className="grid grid-cols-[2rem_1fr_5.5rem] items-center gap-2 text-muted-foreground">
        <span className="tabular-nums">{index + 1}</span>
        <div className="h-1.5 rounded-full bg-muted" aria-label={t("details.connectionIdle", { n: index + 1 })} role="img" />
        <span className="text-right">{t("details.idle")}</span>
      </li>
    );
  }
  const len = c.end - c.start;
  const value = pct(c.position - c.start, len);
  return (
    <li className="grid grid-cols-[2rem_1fr_5.5rem] items-center gap-2" title={t("details.bytes", { from: c.start, to: c.end - 1 })}>
      <span className="tabular-nums text-muted-foreground">{index + 1}</span>
      <Progress
        value={value}
        className="h-1.5"
        aria-label={t("details.connectionBar", { n: index + 1, percent: Math.round(value), size: formatBytes(len) })}
      />
      <span className="text-right tabular-nums">{formatSpeedOrZero(c.speed)}</span>
    </li>
  );
}

export function DownloadDetails({ d }: { d: Download }) {
  const running = d.status === "downloading";
  const left = running && d.size !== null && d.speed > 0 ? (d.size - d.received) / d.speed : null;
  const folder = d.path.replace(/[\\/][^\\/]*$/, "");
  return (
    <section aria-label={t("details.label", { name: d.name })} className="flex h-56 shrink-0 flex-col gap-3 overflow-hidden border-t bg-muted/20 p-3 text-sm">
      <div className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
        <span className="text-muted-foreground">{t("details.file")}</span>
        <span className="truncate" title={d.path}>{d.name}</span>
        <span className="text-muted-foreground">{t("details.folder")}</span>
        <span className="truncate" title={folder}>{folder}</span>
        <span className="text-muted-foreground">{t("details.link")}</span>
        <span className="truncate" title={d.url}>{d.url}</span>
        <span className="text-muted-foreground">{t("details.progress")}</span>
        <span className="tabular-nums">
          {t("details.ofSize", { received: formatBytes(d.received), size: formatBytes(d.size) })}
          {running && ` · ${formatSpeedOrZero(d.speed)}`}
          {left !== null && ` · ${t("details.left", { time: formatTimeLeft(left) })}`}
        </span>
        {d.error && (
          <>
            <span className="text-muted-foreground">{t("details.problem")}</span>
            <span className={d.status === "failed" ? "text-destructive" : ""}>{msgText(d.error)}</span>
          </>
        )}
      </div>
      {d.status === "failed" && d.errorCode === VIDEO_OUTDATED && <UpdateVideoComponent d={d} />}
      {cannotPause(d) && (
        <p className="flex items-center gap-2 text-xs text-amber-700 dark:text-amber-300">
          <Ban className="size-3.5 shrink-0" aria-hidden /> {cannotPauseText()}
        </p>
      )}
      {running && d.kind === "video" ? (
        <p className="text-xs text-muted-foreground">
          {d.merging ? t("details.joining") : t("details.videoStream")}
        </p>
      ) : running && d.connections.length > 0 ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <h3 className="mb-1.5 text-xs font-medium text-muted-foreground">
            {t("details.connections", { count: d.connections.filter(Boolean).length })}
          </h3>
          <ol className="grid grid-cols-1 gap-x-6 gap-y-1.5 text-xs md:grid-cols-2">
            {d.connections.map((c, i) => (
              <ConnectionRow key={i} index={i} c={c} />
            ))}
          </ol>
        </div>
      ) : (
        running && <p className="text-xs text-muted-foreground">{t("details.connecting")}</p>
      )}
    </section>
  );
}
