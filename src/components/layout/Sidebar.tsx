import { useMemo } from "react";
import type { LucideIcon } from "lucide-react";
import { CheckCircle2, Clock, Download, File, Film, Image, Layers, Music } from "lucide-react";
import { cn } from "@/lib/utils";
import { type Download as Dl, type Filter, matches, sameFilter } from "@/features/downloads/types";

interface Item {
  label: string;
  icon: LucideIcon;
  filter: Filter;
}

const groups: { title?: string; items: Item[] }[] = [
  {
    items: [
      { label: "All downloads", icon: Layers, filter: { kind: "all" } },
      { label: "Unfinished", icon: Download, filter: { kind: "unfinished" } },
      { label: "Finished", icon: CheckCircle2, filter: { kind: "finished" } },
    ],
  },
  {
    title: "Categories",
    items: [
      { label: "Video", icon: Film, filter: { kind: "category", category: "video" } },
      { label: "Audio", icon: Music, filter: { kind: "category", category: "audio" } },
      { label: "Images", icon: Image, filter: { kind: "category", category: "images" } },
      { label: "Other", icon: File, filter: { kind: "category", category: "other" } },
    ],
  },
  {
    title: "Queues",
    items: [{ label: "Main queue", icon: Clock, filter: { kind: "queue", queue: "Main queue" } }],
  },
];

interface Props {
  downloads: Dl[];
  selected: Filter;
  onSelect: (f: Filter) => void;
}

const allItems = groups.flatMap((g) => g.items);

export function Sidebar({ downloads, selected, onSelect }: Props) {
  // One pass over the downloads for every count.
  const counts = useMemo(() => {
    const c = new Map<Item, number>(allItems.map((it) => [it, 0]));
    for (const d of downloads) {
      for (const it of allItems) if (matches(d, it.filter)) c.set(it, (c.get(it) ?? 0) + 1);
    }
    return c;
  }, [downloads]);

  return (
    <nav aria-label="Filters" className="flex w-52 shrink-0 flex-col gap-4 overflow-y-auto border-r bg-sidebar p-2 text-sidebar-foreground">
      {groups.map((g, gi) => (
        <div key={gi} className="flex flex-col gap-0.5">
          {g.title && (
            <h2 className="px-2 pb-1 text-[0.7rem] font-semibold tracking-wide text-muted-foreground uppercase">{g.title}</h2>
          )}
          {g.items.map((it) => {
            const active = sameFilter(it.filter, selected);
            const count = counts.get(it) ?? 0;
            return (
              <button
                key={it.label}
                type="button"
                aria-current={active ? "page" : undefined}
                onClick={() => onSelect(it.filter)}
                className={cn(
                  "flex h-8 items-center gap-2 rounded-md px-2 text-left text-sm outline-none transition-colors duration-150",
                  "hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-sidebar-ring",
                  active && "bg-sidebar-accent font-medium text-sidebar-primary",
                )}
              >
                <it.icon className="size-4 shrink-0" strokeWidth={1.75} aria-hidden />
                <span className="flex-1 truncate">{it.label}</span>
                <span className="text-xs tabular-nums text-muted-foreground">{count}</span>
              </button>
            );
          })}
        </div>
      ))}
    </nav>
  );
}
