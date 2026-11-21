import { useEffect, useMemo, useState, type KeyboardEvent } from "react";
import { ArrowDown, ArrowUp, Ban, Check, ChevronDown, ChevronRight, Clock, Copy, ExternalLink, FolderOpen, ListVideo, Pause, Play, Trash2 } from "lucide-react";
import { Progress } from "@/components/ui/progress";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { cn } from "@/lib/utils";
import { msgText, t } from "@/lib/i18n";
import { cannotPauseText, type Download, type Queue, type Status, canPause, canResume, cannotPause, isMkvVideo } from "./types";
import { formatBytes, formatDate, formatSpeed, formatTimeLeft } from "./format";
import { groupRows } from "./playlist";

type SortKey = "name" | "size" | "status" | "speed" | "timeLeft" | "addedAt";

// Functions, not constants: the labels are looked up in the current language.
const columns = (): { key: SortKey; label: string; className: string }[] => [
  { key: "name", label: t("list.name"), className: "w-auto" },
  { key: "size", label: t("list.size"), className: "w-24 text-right" },
  { key: "status", label: t("list.status"), className: "w-48" },
  { key: "speed", label: t("list.speed"), className: "w-28 text-right" },
  { key: "timeLeft", label: t("list.timeLeft"), className: "w-24 text-right" },
  { key: "addedAt", label: t("list.added"), className: "w-44" },
];

export function statusLabel(s: Status): string {
  return s === "waiting" ? t("waiting") : t(`status.${s as Exclude<Status, "waiting">}`);
}

/** What the row menu (and double-click) can do; each acts on the download's id. */
export interface RowActions {
  open: (d: Download) => void;
  openFolder: (d: Download) => void;
  resume: (d: Download) => void;
  pause: (d: Download) => void;
  copyLink: (d: Download) => void;
  moveToQueue: (d: Download, queue: number | null) => void;
  remove: (d: Download) => void;
  /** A whole playlist: resume (or retry) every entry, or pause every running one. */
  resumePlaylist: (playlist: number) => void;
  pausePlaylist: (playlist: number) => void;
}

const rowId = (id: number) => `download-row-${id}`;

/** A row of the list: a download (indented under its playlist), or a playlist heading. */
type Item =
  | { d: Download; indent: boolean }
  | { heading: { id: number; title: string; rows: Download[]; open: boolean } };

/** A playlist's heading row: its title, how many entries finished, their combined
 * speed, and Pause / Resume for the whole playlist. Not a download: never selected. */
function PlaylistHeading(props: { id: number; title: string; rows: Download[]; open: boolean; onFold: () => void; actions: RowActions }) {
  const { id, title, rows, open, onFold, actions } = props;
  const done = rows.filter((r) => r.status === "finished").length;
  const failed = rows.filter((r) => r.status === "failed").length;
  const speed = rows.reduce((n, r) => n + r.speed, 0);
  const pct = rows.length ? (done / rows.length) * 100 : 0;
  const Fold = open ? ChevronDown : ChevronRight;
  return (
    <tr className="h-9 border-b border-border/60 bg-muted/40">
      <td className="px-3">
        <span className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={onFold}
            aria-expanded={open}
            aria-label={open ? t("playlist.collapse") : t("playlist.expand")}
            className="rounded-sm outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
          >
            <Fold className="size-4" aria-hidden />
          </button>
          <ListVideo className="size-4 shrink-0" aria-hidden />
          <span className="truncate font-medium" title={title}>
            {title}
          </span>
          <span className="ml-auto flex shrink-0 gap-0.5">
            <button
              type="button"
              disabled={!rows.some(canResume)}
              onClick={() => actions.resumePlaylist(id)}
              title={t("playlist.resumeHint")}
              aria-label={t("playlist.resume")}
              className="rounded-sm p-1 outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-40"
            >
              <Play className="size-3.5" aria-hidden />
            </button>
            <button
              type="button"
              disabled={!rows.some(canPause)}
              onClick={() => actions.pausePlaylist(id)}
              aria-label={t("playlist.pause")}
              title={t("playlist.pause")}
              className="rounded-sm p-1 outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-40"
            >
              <Pause className="size-3.5" aria-hidden />
            </button>
          </span>
        </span>
      </td>
      <td className="px-3 text-right text-xs tabular-nums text-muted-foreground">{t("playlist.count", { count: rows.length })}</td>
      <td className="px-3">
        <div className="flex items-center gap-2">
          <Progress value={pct} className="h-1.5 flex-1" aria-label={t("playlist.done", { done, total: rows.length })} />
          <span className={cn("w-14 truncate text-xs tabular-nums", failed ? "text-destructive" : "text-muted-foreground")}>
            {done}/{rows.length}
          </span>
        </div>
      </td>
      <td className="px-3 text-right tabular-nums">{formatSpeed(speed)}</td>
      <td className="px-3" />
      <td className="px-3" />
    </tr>
  );
}

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
  queues: Queue[];
  loaded: boolean;
}

export function DownloadList({ downloads, selectedId, onSelect, actions, queues, loaded }: Props) {
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

  // Playlist entries are shown under their playlist's heading; a heading can be folded.
  const [folded, setFolded] = useState<Set<number>>(new Set());
  const items = useMemo(
    () =>
      groupRows(rows).flatMap((g): Item[] => {
        if (!g.group) return g.rows.map((d) => ({ d, indent: false }));
        const open = !folded.has(g.group.id);
        return [{ heading: { id: g.group.id, title: g.group.title, rows: g.rows, open } }, ...(open ? g.rows.map((d) => ({ d, indent: true })) : [])];
      }),
    [rows, folded],
  );
  /** The rows the arrow keys move through: folded playlists' entries are left out. */
  const shown = useMemo(() => items.flatMap((i) => ("d" in i ? [i.d] : [])), [items]);
  const fold = (id: number) =>
    setFolded((f) => {
      const next = new Set(f);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  const selected = rows.find((r) => r.id === selectedId) ?? null;

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Enter" && selected?.status === "finished") {
      e.preventDefault();
      actions.open(selected);
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    if (shown.length === 0) return;
    const i = shown.findIndex((r) => r.id === selectedId);
    const next = e.key === "ArrowDown" ? Math.min(shown.length - 1, i + 1) : Math.max(0, i - 1);
    onSelect(shown[next < 0 ? 0 : next].id);
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
        aria-label={t("list.label")}
        aria-activedescendant={selectedId ? rowId(selectedId) : undefined}
        tabIndex={0}
        onKeyDown={onKey}
        className="w-full table-fixed border-collapse text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
      >
        <thead className="sticky top-0 z-10 bg-background">
          <tr className="border-b">
            {columns().map((c) => {
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
          {items.map((item) => {
            if ("heading" in item) {
              const h = item.heading;
              return <PlaylistHeading key={`playlist-${h.id}`} {...h} onFold={() => fold(h.id)} actions={actions} />;
            }
            const { d, indent } = item;
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
                <td className={cn("px-3", indent && "pl-9")} title={d.url}>
                  <span className="flex items-center gap-1.5">
                    <span className="truncate">{d.name}</span>
                    {cannotPause(d) && (
                      <span title={cannotPauseText()} className="flex shrink-0 items-center gap-1 text-xs text-amber-700 dark:text-amber-300">
                        <Ban className="size-3.5" aria-hidden />
                        {t("list.cannotPause")}
                      </span>
                    )}
                    {isMkvVideo(d) && (
                      <span className="shrink-0 rounded border px-1 text-xs text-muted-foreground" title={t("list.mkvHint")}>
                        {t("list.mkv")}
                      </span>
                    )}
                  </span>
                </td>
                <td className="px-3 text-right tabular-nums">{formatBytes(d.size)}</td>
                <td className="px-3">
                  {d.status === "downloading" && d.merging ? (
                    <div className="flex items-center gap-2">
                      <div role="progressbar" aria-label={t("list.joining")} className="progress-indeterminate h-1.5 flex-1 overflow-hidden rounded-full bg-muted" />
                      <span className="w-14 text-xs text-muted-foreground">{t("list.merging")}</span>
                    </div>
                  ) : d.status === "downloading" && d.size === null ? (
                    <div className="flex items-center gap-2">
                      <div role="progressbar" aria-label={t("list.sizeUnknown")} className="progress-indeterminate h-1.5 flex-1 overflow-hidden rounded-full bg-muted" />
                      <span className="w-14 text-xs text-muted-foreground">{formatBytes(d.received)}</span>
                    </div>
                  ) : d.status === "downloading" || d.status === "paused" || (d.status === "waiting" && d.received > 0) ? (
                    <div className="flex items-center gap-2">
                      <Progress value={pct} className="h-1.5 flex-1" aria-label={t("list.progress", { status: statusLabel(d.status), percent: Math.round(pct) })} />
                      <span className="w-14 text-xs tabular-nums text-muted-foreground">
                        {d.status === "downloading" ? `${Math.round(pct)}%` : statusLabel(d.status)}
                      </span>
                    </div>
                  ) : (
                    <span
                      title={d.error ? msgText(d.error) : undefined}
                      className={cn("text-xs", d.status === "failed" ? "text-destructive" : "text-muted-foreground")}
                    >
                      {statusLabel(d.status)}
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
        <p className="p-8 text-center text-sm text-muted-foreground">{t("list.empty")}</p>
      )}
    </div>
    </ContextMenuTrigger>
    {selected && (
      <ContextMenuContent className="w-52">
        <ContextMenuItem disabled={selected.status !== "finished"} onSelect={() => actions.open(selected)}>
          <ExternalLink aria-hidden /> {t("common.open")}
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => actions.openFolder(selected)}>
          <FolderOpen aria-hidden /> {t("common.openFolder")}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled={!canResume(selected)} onSelect={() => actions.resume(selected)}>
          <Play aria-hidden /> {t("common.resume")}
        </ContextMenuItem>
        <ContextMenuItem disabled={!canPause(selected)} onSelect={() => actions.pause(selected)}>
          <Pause aria-hidden /> {cannotPause(selected) ? t("list.pauseNotPossible") : t("common.pause")}
        </ContextMenuItem>
        <ContextMenuSub>
          <ContextMenuSubTrigger>
            <Clock aria-hidden /> {t("moveToQueue")}
          </ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-48">
            {[{ id: null as number | null, name: t("mainQueue") }, ...queues].map((q) => (
              <ContextMenuItem key={q.id ?? "main"} onSelect={() => actions.moveToQueue(selected, q.id)}>
                {(selected.queue ?? null) === q.id ? <Check aria-hidden /> : <span className="size-4" aria-hidden />}
                <span className="truncate">{q.name}</span>
              </ContextMenuItem>
            ))}
          </ContextMenuSubContent>
        </ContextMenuSub>
        {selected.playlist && (
          <>
            <ContextMenuItem onSelect={() => selected.playlist && actions.resumePlaylist(selected.playlist.id)}>
              <Play aria-hidden /> {t("playlist.resume")}
            </ContextMenuItem>
            <ContextMenuItem onSelect={() => selected.playlist && actions.pausePlaylist(selected.playlist.id)}>
              <Pause aria-hidden /> {t("playlist.pause")}
            </ContextMenuItem>
          </>
        )}
        <ContextMenuItem onSelect={() => actions.copyLink(selected)}>
          <Copy aria-hidden /> {t("list.copyLink")}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem variant="destructive" onSelect={() => actions.remove(selected)}>
          <Trash2 aria-hidden /> {t("common.delete")}
        </ContextMenuItem>
      </ContextMenuContent>
    )}
    </ContextMenu>
  );
}
