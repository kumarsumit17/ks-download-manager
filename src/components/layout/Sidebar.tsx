import { useMemo } from "react";
import type { LucideIcon } from "lucide-react";
import { CheckCircle2, Clock, Download, File, Film, Image, Layers, Music, Play, Square } from "lucide-react";
import { cn } from "@/lib/utils";
import { t } from "@/lib/text";
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger } from "@/components/ui/context-menu";
import { type Download as Dl, type Filter, type Queue, matches, sameFilter } from "@/features/downloads/types";

interface Item {
  key: string;
  label: string;
  icon: LucideIcon;
  filter: Filter;
  /** For queues: the queue to start or stop from the item's menu. */
  queue?: number | null;
}

const fixed: { title?: string; items: Item[] }[] = [
  {
    items: [
      { key: "all", label: "All downloads", icon: Layers, filter: { kind: "all" } },
      { key: "unfinished", label: "Unfinished", icon: Download, filter: { kind: "unfinished" } },
      { key: "finished", label: "Finished", icon: CheckCircle2, filter: { kind: "finished" } },
    ],
  },
  {
    title: "Categories",
    items: [
      { key: "video", label: t("categoryVideo"), icon: Film, filter: { kind: "category", category: "video" } },
      { key: "audio", label: t("categoryAudio"), icon: Music, filter: { kind: "category", category: "audio" } },
      { key: "images", label: t("categoryImages"), icon: Image, filter: { kind: "category", category: "images" } },
      { key: "other", label: t("categoryOther"), icon: File, filter: { kind: "category", category: "other" } },
    ],
  },
];

interface Props {
  downloads: Dl[];
  queues: Queue[];
  selected: Filter;
  onSelect: (f: Filter) => void;
  onStartQueue: (queue: number | null) => void;
  onStopQueue: (queue: number | null) => void;
}

export function Sidebar({ downloads, queues, selected, onSelect, onStartQueue, onStopQueue }: Props) {
  const groups = useMemo(
    () => [
      ...fixed,
      {
        title: t("queues"),
        items: [
          { key: "q-main", label: t("mainQueue"), icon: Clock, filter: { kind: "queue", queue: null } as Filter, queue: null },
          ...queues.map((q) => ({ key: `q-${q.id}`, label: q.name, icon: Clock, filter: { kind: "queue", queue: q.id } as Filter, queue: q.id })),
        ],
      },
    ],
    [queues],
  );

  // One pass over the downloads for every count.
  const counts = useMemo(() => {
    const all = groups.flatMap((g) => g.items);
    const c = new Map<string, number>(all.map((it) => [it.key, 0]));
    for (const d of downloads) {
      for (const it of all) if (matches(d, it.filter)) c.set(it.key, (c.get(it.key) ?? 0) + 1);
    }
    return c;
  }, [downloads, groups]);

  const button = (it: Item) => {
    const active = sameFilter(it.filter, selected);
    return (
      <button
        type="button"
        aria-current={active ? "page" : undefined}
        onClick={() => onSelect(it.filter)}
        className={cn(
          "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-sm outline-none transition-colors duration-150",
          "hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-sidebar-ring",
          active && "bg-sidebar-accent font-medium text-sidebar-primary",
        )}
      >
        <it.icon className="size-4 shrink-0" strokeWidth={1.75} aria-hidden />
        <span className="flex-1 truncate">{it.label}</span>
        <span className="text-xs tabular-nums text-muted-foreground">{counts.get(it.key) ?? 0}</span>
      </button>
    );
  };

  return (
    <nav aria-label="Filters" className="flex w-52 shrink-0 flex-col gap-4 overflow-y-auto border-r bg-sidebar p-2 text-sidebar-foreground">
      {groups.map((g, gi) => (
        <div key={gi} className="flex flex-col gap-0.5">
          {g.title && (
            <h2 className="px-2 pb-1 text-[0.7rem] font-semibold tracking-wide text-muted-foreground uppercase">{g.title}</h2>
          )}
          {g.items.map((it: Item) =>
            it.queue === undefined ? (
              <div key={it.key}>{button(it)}</div>
            ) : (
              // Queues: right-click to start or stop; the Queues dialog has the same actions.
              <ContextMenu key={it.key}>
                <ContextMenuTrigger asChild>
                  <div>{button(it)}</div>
                </ContextMenuTrigger>
                <ContextMenuContent className="w-44">
                  <ContextMenuItem onSelect={() => onStartQueue(it.queue ?? null)}>
                    <Play aria-hidden /> {t("start")}
                  </ContextMenuItem>
                  <ContextMenuItem onSelect={() => onStopQueue(it.queue ?? null)}>
                    <Square aria-hidden /> {t("stop")}
                  </ContextMenuItem>
                </ContextMenuContent>
              </ContextMenu>
            ),
          )}
        </div>
      ))}
    </nav>
  );
}
