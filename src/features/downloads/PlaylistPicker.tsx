import { useEffect, useRef, useState } from "react";
import { Ban, ListVideo } from "lucide-react";
import { t } from "@/lib/i18n";
import { api } from "./api";
import { clickEntry, formatDuration, imageType, toggleAll } from "./playlist";
import type { PlaylistView } from "./types";

interface Props {
  /** The playlist link, as looked up (Rust finds the thumbnails by it). */
  url: string;
  playlist: PlaylistView;
  ticked: Set<number>;
  onTicked: (ticked: Set<number>) => void;
}

/**
 * The playlist picker (specs/video-playlists.md): every available entry ticked when it
 * opens, Select all with a count, shift-click for ranges, title, duration and thumbnail
 * per entry, unavailable entries marked and never ticked.
 */
export function PlaylistPicker({ url, playlist, ticked, onTicked }: Props) {
  const anchor = useRef<number | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const available = playlist.entries.filter((e) => !e.unavailable).length;
  const all = available > 0 && ticked.size === available;

  return (
    <div className="grid gap-2">
      <div className="flex items-center gap-2 text-sm">
        <ListVideo className="size-4 shrink-0" aria-hidden />
        <span className="truncate font-medium" title={playlist.title}>
          {playlist.title}
        </span>
      </div>
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          className="size-4 accent-primary"
          checked={all}
          ref={(el) => {
            if (el) el.indeterminate = ticked.size > 0 && !all;
          }}
          onChange={() => {
            onTicked(toggleAll(playlist.entries, ticked));
            anchor.current = null;
          }}
        />
        {t("playlist.selectAll")}
        <span className="ml-auto text-xs tabular-nums text-muted-foreground" aria-live="polite">
          {t("playlist.selected", { count: ticked.size, total: available })}
        </span>
      </label>
      <div ref={scroller} role="group" aria-label={t("playlist.entries")} className="max-h-64 overflow-auto rounded-md border">
        <ul>
          {playlist.entries.map((e) => (
            <li key={e.index} className="border-b border-border/60 last:border-b-0">
              <label
                className={
                  "flex items-center gap-3 px-2 py-1.5 text-sm " + (e.unavailable ? "text-muted-foreground" : "hover:bg-muted/60")
                }
                title={e.url}
              >
                <input
                  type="checkbox"
                  className="size-4 shrink-0 accent-primary"
                  disabled={e.unavailable}
                  checked={ticked.has(e.index)}
                  onChange={() => {}}
                  onClick={(ev) => {
                    const next = clickEntry(playlist.entries, ticked, anchor.current, e.index, ev.shiftKey);
                    anchor.current = next.anchor;
                    onTicked(next.ticked);
                  }}
                />
                <Thumbnail url={url} index={e.index} show={e.hasThumbnail && !e.unavailable} root={scroller} />
                <span className="w-7 shrink-0 text-right text-xs tabular-nums text-muted-foreground">{e.index}</span>
                <span className="min-w-0 flex-1 truncate">{e.title || t("common.unknown")}</span>
                {e.unavailable ? (
                  <span className="flex shrink-0 items-center gap-1 text-xs">
                    <Ban className="size-3.5" aria-hidden /> {t("playlist.unavailable")}
                  </span>
                ) : (
                  <span className="shrink-0 text-xs tabular-nums text-muted-foreground">{formatDuration(e.duration)}</span>
                )}
              </label>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

/** Loads one thumbnail through Rust when it scrolls into view, as a data URL. */
function Thumbnail({ url, index, show, root }: { url: string; index: number; show: boolean; root: React.RefObject<HTMLDivElement | null> }) {
  const [src, setSrc] = useState<string | null>(null);
  const box = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!show || !box.current) return;
    let cancelled = false;
    const load = () => {
      api.thumbnail(url, index).then(
        (buf) => {
          const bytes = new Uint8Array(buf);
          const type = imageType(bytes);
          if (!type || cancelled) return;
          const reader = new FileReader();
          reader.onload = () => !cancelled && typeof reader.result === "string" && setSrc(reader.result);
          reader.readAsDataURL(new Blob([bytes], { type }));
        },
        () => {},
      );
    };
    const seen = new IntersectionObserver(
      (items) => {
        if (items.some((i) => i.isIntersecting)) {
          seen.disconnect();
          load();
        }
      },
      { root: root.current, rootMargin: "200px" },
    );
    seen.observe(box.current);
    return () => {
      cancelled = true;
      seen.disconnect();
    };
  }, [url, index, show, root]);

  return (
    <span ref={box} className="block h-9 w-16 shrink-0 overflow-hidden rounded bg-muted" aria-hidden>
      {src && <img src={src} alt="" className="size-full object-cover" />}
    </span>
  );
}
