import { useMemo, useState, type KeyboardEvent } from "react";
import { ArrowDown, ArrowUp } from "lucide-react";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import type { Download, Status } from "./types";
import { formatBytes, formatDate, formatSpeed, formatTimeLeft } from "./format";

type SortKey = "name" | "size" | "status" | "speed" | "timeLeft" | "addedAt";

const columns: { key: SortKey; label: string; className: string }[] = [
  { key: "name", label: "Name", className: "w-auto" },
  { key: "size", label: "Size", className: "w-24 text-right" },
  { key: "status", label: "Status", className: "w-48" },
  { key: "speed", label: "Speed", className: "w-28 text-right" },
  { key: "timeLeft", label: "Time left", className: "w-24 text-right" },
  { key: "addedAt", label: "Date added", className: "w-44" },
];

const statusLabel: Record<Status, string> = {
  downloading: "Downloading",
  paused: "Paused",
  queued: "Queued",
  finished: "Finished",
  failed: "Failed",
};

function timeLeft(d: Download): number | null {
  if (d.status !== "downloading" || d.speed <= 0 || d.size === null) return null;
  return (d.size - d.received) / d.speed;
}

function sortValue(d: Download, k: SortKey): number | string {
  switch (k) {
    case "name":
      return d.name.toLowerCase();
    case "size":
      return d.size ?? -1;
    case "status":
      return d.status;
    case "speed":
      return d.speed;
    case "timeLeft":
      return timeLeft(d) ?? Number.MAX_SAFE_INTEGER;
    case "addedAt":
      return d.addedAt;
  }
}

interface Props {
  downloads: Download[];
  selectedId: string | null;
  onSelect: (id: string | null) => void;
}

export function DownloadList({ downloads, selectedId, onSelect }: Props) {
  const [sort, setSort] = useState<{ key: SortKey; asc: boolean }>({ key: "addedAt", asc: false });

  const rows = useMemo(() => {
    const out = [...downloads];
    out.sort((a, b) => {
      const x = sortValue(a, sort.key);
      const y = sortValue(b, sort.key);
      const c = x < y ? -1 : x > y ? 1 : 0;
      return sort.asc ? c : -c;
    });
    return out;
  }, [downloads, sort]);

  const onKey = (e: KeyboardEvent) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    if (rows.length === 0) return;
    const i = rows.findIndex((r) => r.id === selectedId);
    const next = e.key === "ArrowDown" ? Math.min(rows.length - 1, i + 1) : Math.max(0, i - 1);
    onSelect(rows[next < 0 ? 0 : next].id);
  };

  return (
    <div className="min-h-0 flex-1 overflow-auto outline-none focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-inset" tabIndex={0} onKeyDown={onKey} aria-label="Downloads">
      <table className="w-full table-fixed border-collapse text-sm" aria-rowcount={rows.length}>
        <thead className="sticky top-0 z-10 bg-background">
          <tr className="border-b">
            {columns.map((c) => {
              const active = sort.key === c.key;
              return (
                <th
                  key={c.key}
                  scope="col"
                  aria-sort={active ? (sort.asc ? "ascending" : "descending") : "none"}
                  className={cn("h-8 px-3 font-medium text-muted-foreground", c.className)}
                >
                  <button
                    type="button"
                    tabIndex={-1}
                    onClick={() => setSort({ key: c.key, asc: active ? !sort.asc : true })}
                    className={cn("inline-flex items-center gap-1 hover:text-foreground", c.className.includes("text-right") && "flex-row-reverse")}
                  >
                    {c.label}
                    {active && (sort.asc ? <ArrowUp className="size-3" aria-hidden /> : <ArrowDown className="size-3" aria-hidden />)}
                  </button>
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {rows.map((d) => {
            const pct = d.size ? Math.min(100, (d.received / d.size) * 100) : 0;
            const selected = d.id === selectedId;
            return (
              <tr
                key={d.id}
                aria-selected={selected}
                onClick={() => onSelect(d.id)}
                className={cn(
                  "h-9 border-b border-border/60 transition-colors duration-120",
                  selected ? "bg-primary/12" : "hover:bg-muted/60",
                )}
              >
                <td className="truncate px-3" title={d.url}>{d.name}</td>
                <td className="px-3 text-right tabular-nums">{formatBytes(d.size)}</td>
                <td className="px-3">
                  {d.status === "downloading" || d.status === "paused" ? (
                    <div className="flex items-center gap-2">
                      <Progress value={pct} className="h-1.5 flex-1" aria-label={`${statusLabel[d.status]} ${Math.round(pct)}%`} />
                      <span className="w-14 text-xs tabular-nums text-muted-foreground">
                        {d.status === "paused" ? "Paused" : `${Math.round(pct)}%`}
                      </span>
                    </div>
                  ) : (
                    <span className={cn("text-xs", d.status === "failed" ? "text-destructive" : "text-muted-foreground")}>
                      {statusLabel[d.status]}
                    </span>
                  )}
                </td>
                <td className="px-3 text-right tabular-nums">{formatSpeed(d.speed)}</td>
                <td className="px-3 text-right tabular-nums">{formatTimeLeft(timeLeft(d))}</td>
                <td className="truncate px-3 text-muted-foreground">{formatDate(d.addedAt)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {rows.length === 0 && (
        <p className="p-8 text-center text-sm text-muted-foreground">No downloads here yet.</p>
      )}
    </div>
  );
}
