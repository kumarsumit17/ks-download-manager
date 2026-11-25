import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Ban, ListVideo } from "lucide-react";
import { t } from "@/lib/i18n";
import { api } from "./api";
import { clickEntry, formatDuration, imageType, toggleAll } from "./playlist";
import { visibleRange } from "./windowing";
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
 *
 * Windowed like the download list: only the entries in view (plus a few) are drawn, so a
 * playlist of thousands opens at once. A thumbnail is asked for when its entry is drawn;
 * no observer is needed, and Rust fetches at most 4 at a time.
 */
export function PlaylistPicker({ url, playlist, ticked, onTicked }: Props) {
  const anchor = useRef<number | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [rowHeight, setRowHeight] = useState(49);
  const available = playlist.entries.filter((e) => !e.unavailable).length;
  const all = available > 0 && ticked.size === available;

  // The real row height, measured from a drawn entry.
  useLayoutEffect(() => {
    const h = scroller.current?.querySelector<HTMLElement>("li[data-entry]")?.getBoundingClientRect().height;
    if (h && Math.abs(h - rowHeight) > 0.1) setRowHeight(h);
  });
  const viewport = scroller.current?.clientHeight ?? 256;
  const range = visibleRange(playlist.entries.length, rowHeight, scrollTop, viewport);

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
      <div
        ref={scroller}
        role="group"
        aria-label={t("playlist.entries")}
        onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
        className="max-h-64 overflow-auto rounded-md border"
      >
        <ul>
          {range.before > 0 && <li aria-hidden style={{ height: range.before }} />}
          {playlist.entries.slice(range.start, range.end).map((e) => (
            <li key={e.index} data-entry className="border-b border-border/60 last:border-b-0">
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
                <Thumbnail url={url} index={e.index} show={e.hasThumbnail && !e.unavailable} />
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
          {range.after > 0 && <li aria-hidden style={{ height: range.after }} />}
        </ul>
      </div>
    </div>
  );
}

/** Thumbnails already loaded, so scrolling back does not fetch them again. Capped. */
const loaded = new Map<string, string>();
const LOADED_MAX = 300;

function remember(key: string, src: string) {
  loaded.set(key, src);
  if (loaded.size > LOADED_MAX) {
    const oldest = loaded.keys().next().value;
    if (oldest !== undefined) loaded.delete(oldest);
  }
}

/** One entry's thumbnail, fetched through Rust when the entry is drawn, as a data URL. */
function Thumbnail({ url, index, show }: { url: string; index: number; show: boolean }) {
  const key = `${url}#${index}`;
  const [src, setSrc] = useState<string | null>(() => loaded.get(key) ?? null);

  useEffect(() => {
    if (!show || loaded.has(key)) return;
    let cancelled = false;
    api.thumbnail(url, index).then(
      (buf) => {
        const bytes = new Uint8Array(buf);
        const type = imageType(bytes);
        if (!type) return;
        const reader = new FileReader();
        reader.onload = () => {
          if (typeof reader.result !== "string") return;
          remember(key, reader.result);
          if (!cancelled) setSrc(reader.result);
        };
        reader.readAsDataURL(new Blob([bytes], { type }));
      },
      () => {},
    );
    return () => {
      cancelled = true;
    };
  }, [url, index, show, key]);

  return (
    <span className="block h-9 w-16 shrink-0 overflow-hidden rounded bg-muted" aria-hidden>
      {src && <img src={src} alt="" className="size-full object-cover" />}
    </span>
  );
}
