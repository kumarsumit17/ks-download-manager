import { Ban } from "lucide-react";
import { Progress } from "@/components/ui/progress";
import { formatBytes, formatSpeed, formatTimeLeft } from "./format";
import { CANNOT_PAUSE, type ConnectionTick, type Download, cannotPause } from "./types";

function pct(done: number, total: number): number {
  return total > 0 ? Math.min(100, (done / total) * 100) : 0;
}

/** One bar per connection: how far it is through its current range. */
function ConnectionRow({ index, c }: { index: number; c: ConnectionTick | null }) {
  const label = `Connection ${index + 1}`;
  if (!c) {
    return (
      <li className="grid grid-cols-[2rem_1fr_5.5rem] items-center gap-2 text-muted-foreground">
        <span className="tabular-nums">{index + 1}</span>
        <div className="h-1.5 rounded-full bg-muted" aria-label={`${label}: idle`} role="img" />
        <span className="text-right">idle</span>
      </li>
    );
  }
  const len = c.end - c.start;
  const value = pct(c.position - c.start, len);
  return (
    <li className="grid grid-cols-[2rem_1fr_5.5rem] items-center gap-2" title={`Bytes ${c.start.toLocaleString()}–${(c.end - 1).toLocaleString()}`}>
      <span className="tabular-nums text-muted-foreground">{index + 1}</span>
      <Progress
        value={value}
        className="h-1.5"
        aria-label={`${label}: ${Math.round(value)}% of ${formatBytes(len)}`}
      />
      <span className="text-right tabular-nums">{formatSpeed(c.speed) || "0 B/s"}</span>
    </li>
  );
}

export function DownloadDetails({ d }: { d: Download }) {
  const running = d.status === "downloading";
  const left = running && d.size !== null && d.speed > 0 ? (d.size - d.received) / d.speed : null;
  const folder = d.path.replace(/[\\/][^\\/]*$/, "");
  return (
    <section aria-label={`Details for ${d.name}`} className="flex h-56 shrink-0 flex-col gap-3 overflow-hidden border-t bg-muted/20 p-3 text-sm">
      <div className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
        <span className="text-muted-foreground">File</span>
        <span className="truncate" title={d.path}>{d.name}</span>
        <span className="text-muted-foreground">Folder</span>
        <span className="truncate" title={folder}>{folder}</span>
        <span className="text-muted-foreground">Link</span>
        <span className="truncate" title={d.url}>{d.url}</span>
        <span className="text-muted-foreground">Progress</span>
        <span className="tabular-nums">
          {formatBytes(d.received)} of {formatBytes(d.size)}
          {running && ` · ${formatSpeed(d.speed) || "0 B/s"}`}
          {left !== null && ` · ${formatTimeLeft(left)} left`}
        </span>
        {d.error && (
          <>
            <span className="text-muted-foreground">Problem</span>
            <span className={d.status === "failed" ? "text-destructive" : ""}>{d.error}</span>
          </>
        )}
      </div>
      {cannotPause(d) && (
        <p className="flex items-center gap-2 text-xs text-amber-700 dark:text-amber-300">
          <Ban className="size-3.5 shrink-0" aria-hidden /> {CANNOT_PAUSE}
        </p>
      )}
      {running && d.connections.length > 0 ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <h3 className="mb-1.5 text-xs font-medium text-muted-foreground">
            Connections ({d.connections.filter(Boolean).length} working)
          </h3>
          <ol className="grid grid-cols-1 gap-x-6 gap-y-1.5 text-xs md:grid-cols-2">
            {d.connections.map((c, i) => (
              <ConnectionRow key={i} index={i} c={c} />
            ))}
          </ol>
        </div>
      ) : (
        running && <p className="text-xs text-muted-foreground">Connecting…</p>
      )}
    </section>
  );
}
