import { useEffect, useMemo, useState, type KeyboardEvent } from "react";
import { ArrowDown, ArrowUp, Ban, Copy, ExternalLink, FolderOpen, Pause, Play, Trash2 } from "lucide-react";
import { Progress } from "@/components/ui/progress";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { cn } from "@/lib/utils";
import { CANNOT_PAUSE, type Download, type Status, canPause, canResume, cannotPause } from "./types";
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
  interrupted: "Interrupted",
};

/** What the row menu (and double-click) can do; each acts on the download's id. */
export interface RowActions {
  open: (d: Download) => void;
  openFolder: (d: Download) => void;
  resume: (d: Download) => void;
  pause: (d: Download) => void;
  copyLink: (d: Download) => void;
  remove: (d: Download) => void;
}

const rowId = (id: number) => `download-row-${id}`;

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
  selectedId: number | null;
  onSelect: (id: number | null) => void;
  actions: RowActions;
  loaded: boolean;
}

export function DownloadList({ downloads, selectedId, onSelect, actions, loaded }: Props) {
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

  const selected = rows.find((r) => r.id === selectedId) ?? null;

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Enter" && selected?.status === "finished") {
      e.preventDefault();
      actions.open(selected);
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    if (rows.length === 0) return;
    const i = rows.findIndex((r) => r.id === selectedId);
    const next = e.key === "ArrowDown" ? Math.min(rows.length - 1, i + 1) : Math.max(0, i - 1);
    onSelect(rows[next < 0 ? 0 : next].id);
  };

  // Keep the selected row visible when it changes (arrow keys, sorting, filtering).
  useEffect(() => {
    if (selectedId) document.getElementById(rowId(selectedId))?.scrollIntoView({ block: "nearest" });
  }, [selectedId, rows]);

  return (
    <ContextMenu>
    <ContextMenuTrigger asChild>
    <div className="min-h-0 flex-1 overflow-auto">
      <table
        role="grid"
        aria-label="Downloads"
        aria-activedescendant={selectedId ? rowId(selectedId) : undefined}
        tabIndex={0}
        onKeyDown={onKey}
        className="w-full table-fixed border-collapse text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
      >
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
                    onClick={() => setSort({ key: c.key, asc: active ? !sort.asc : true })}
                    className={cn(
                      "inline-flex items-center gap-1 rounded-sm outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
                      c.className.includes("text-right") && "flex-row-reverse",
                    )}
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
                id={rowId(d.id)}
                aria-selected={selected}
                onClick={() => onSelect(d.id)}
                onContextMenu={() => onSelect(d.id)}
                onDoubleClick={() => d.status === "finished" && actions.open(d)}
                className={cn(
                  "h-9 scroll-mt-8 border-b border-border/60 transition-colors duration-120",
                  selected ? "bg-primary/12" : "hover:bg-muted/60",
                )}
              >
                <td className="px-3" title={d.url}>
                  <span className="flex items-center gap-1.5">
                    <span className="truncate">{d.name}</span>
                    {cannotPause(d) && (
                      <span title={CANNOT_PAUSE} className="flex shrink-0 items-center gap-1 text-xs text-amber-700 dark:text-amber-300">
                        <Ban className="size-3.5" aria-hidden />
                        Can't pause
                      </span>
                    )}
                  </span>
                </td>
                <td className="px-3 text-right tabular-nums">{formatBytes(d.size)}</td>
                <td className="px-3">
                  {d.status === "downloading" && d.size === null ? (
                    <div className="flex items-center gap-2">
                      <div role="progressbar" aria-label="Downloading, size unknown" className="progress-indeterminate h-1.5 flex-1 overflow-hidden rounded-full bg-muted" />
                      <span className="w-14 text-xs text-muted-foreground">{formatBytes(d.received)}</span>
                    </div>
                  ) : d.status === "downloading" || d.status === "paused" ? (
                    <div className="flex items-center gap-2">
                      <Progress value={pct} className="h-1.5 flex-1" aria-label={`${statusLabel[d.status]} ${Math.round(pct)}%`} />
                      <span className="w-14 text-xs tabular-nums text-muted-foreground">
                        {d.status === "paused" ? "Paused" : `${Math.round(pct)}%`}
                      </span>
                    </div>
                  ) : (
                    <span
                      title={d.error ?? undefined}
                      className={cn("text-xs", d.status === "failed" ? "text-destructive" : "text-muted-foreground")}
                    >
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
      {loaded && rows.length === 0 && (
        <p className="p-8 text-center text-sm text-muted-foreground">No downloads here yet.</p>
      )}
    </div>
    </ContextMenuTrigger>
    {selected && (
      <ContextMenuContent className="w-52">
        <ContextMenuItem disabled={selected.status !== "finished"} onSelect={() => actions.open(selected)}>
          <ExternalLink aria-hidden /> Open
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => actions.openFolder(selected)}>
          <FolderOpen aria-hidden /> Open folder
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled={!canResume(selected)} onSelect={() => actions.resume(selected)}>
          <Play aria-hidden /> Resume
        </ContextMenuItem>
        <ContextMenuItem disabled={!canPause(selected)} onSelect={() => actions.pause(selected)}>
          <Pause aria-hidden /> {cannotPause(selected) ? "Pause (not possible)" : "Pause"}
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => actions.copyLink(selected)}>
          <Copy aria-hidden /> Copy link
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem variant="destructive" onSelect={() => actions.remove(selected)}>
          <Trash2 aria-hidden /> Delete
        </ContextMenuItem>
      </ContextMenuContent>
    )}
    </ContextMenu>
  );
}
