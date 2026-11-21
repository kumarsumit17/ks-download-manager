import { useEffect, useRef, useState } from "react";
import { AlertTriangle, Film, FolderOpen, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { t } from "@/lib/i18n";
import { api, message } from "./api";
import { formatBytes } from "./format";
import { allAvailable } from "./playlist";
import { PlaylistPicker } from "./PlaylistPicker";
import type { Category, ProbeView } from "./types";

const categoryLabel = (c: Category) => t(`category.${c}`);

function looksLikeUrl(s: string): boolean {
  try {
    const u = new URL(s.trim());
    return u.protocol === "http:" || u.protocol === "https:";
  } catch {
    return false;
  }
}

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAdded: (id: number) => void;
}

export function AddDownloadDialog({ open, onOpenChange, onAdded }: Props) {
  const [url, setUrl] = useState("");
  const [probe, setProbe] = useState<ProbeView | null>(null);
  const [probing, setProbing] = useState(false);
  const [name, setName] = useState("");
  const [folder, setFolder] = useState("");
  /** The user typed or chose a folder: the category's folder no longer replaces it. */
  const [folderChosen, setFolderChosen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  /** Playlist links: the ticked entries (by position) and the number prefix. */
  const [ticked, setTicked] = useState<Set<number>>(new Set());
  const [numbered, setNumbered] = useState(true);
  const probeFor = useRef("");

  // Fresh dialog each time. The folder starts as the system Downloads folder and becomes
  // the category's folder (Settings) once the link is looked up, unless the user chose one.
  useEffect(() => {
    if (!open) return;
    setUrl("");
    setProbe(null);
    setName("");
    setError(null);
    setBusy(false);
    setFolderChosen(false);
    setNumbered(true);
    api.defaultFolder().then((d) => setFolder(d), () => {});
  }, [open]);

  useEffect(() => {
    if (!probe || folderChosen) return;
    api.categoryFolder(probe.category).then((f) => f && setFolder(f), () => {});
  }, [probe, folderChosen]);

  const choose = async () => {
    try {
      const picked = await api.pickFolder(t("chooseFolder"), folder || null);
      if (picked) {
        setFolder(picked);
        setFolderChosen(true);
      }
    } catch (e) {
      setError(message(e));
    }
  };

  // Looks the link up shortly after it stops changing.
  useEffect(() => {
    const u = url.trim();
    setProbe(null);
    setError(null);
    if (!looksLikeUrl(u)) {
      setProbing(false);
      return;
    }
    setProbing(true);
    probeFor.current = u;
    const timer = setTimeout(() => {
      api.probe(u).then(
        (p) => {
          if (probeFor.current !== u) return;
          setProbe(p);
          setName(p.name);
          setTicked(p.playlist ? allAvailable(p.playlist.entries) : new Set());
          setProbing(false);
        },
        (e) => {
          if (probeFor.current !== u) return;
          setError(message(e));
          setProbing(false);
        },
      );
    }, 400);
    return () => clearTimeout(timer);
  }, [url]);

  const add = async (start: boolean) => {
    setBusy(true);
    setError(null);
    try {
      if (probe?.playlist) {
        const picks = probe.playlist.entries.filter((e) => ticked.has(e.index)).map((e) => ({ index: e.index, url: e.url }));
        await api.addPlaylist(url.trim(), folder, picks, numbered, true);
      } else {
        const r = await api.add(url.trim(), name, folder, start);
        onAdded(r.id);
      }
      onOpenChange(false);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };

  const playlist = probe?.playlist ?? null;
  const ready =
    probe !== null && (playlist ? ticked.size > 0 : name.trim() !== "") && folder.trim() !== "" && !busy;
  const video = probe?.kind === "video" && !playlist;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className={playlist ? "sm:max-w-2xl" : "sm:max-w-lg"}>
        <DialogHeader>
          <DialogTitle>{t("add.title")}</DialogTitle>
          <DialogDescription>{t("add.description")}</DialogDescription>
        </DialogHeader>
        <form
          className="grid gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (ready) void add(true);
          }}
        >
          <div className="grid gap-2">
            <Label htmlFor="add-url">{t("details.link")}</Label>
            <Input
              id="add-url"
              autoFocus
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://"
              spellCheck={false}
              autoComplete="off"
            />
          </div>

          <div aria-live="polite" className="min-h-5 text-sm">
            {probing && (
              <span className="flex items-center gap-2 text-muted-foreground">
                <Loader2 className="size-4 animate-spin motion-reduce:animate-none" aria-hidden /> {t("add.checking")}
              </span>
            )}
            {probe && video && (
              <dl className="grid grid-cols-[6rem_1fr] gap-x-3 gap-y-1">
                <dt className="text-muted-foreground">{t("category.video")}</dt>
                <dd className="flex items-center gap-1.5">
                  <Film className="size-4 shrink-0" aria-hidden /> {t("add.best")}
                </dd>
                <dt className="text-muted-foreground">{t("list.size")}</dt>
                <dd className="tabular-nums">{probe.size === null ? t("common.unknown") : t("add.about", { size: formatBytes(probe.size) })}</dd>
              </dl>
            )}
            {probe && !video && (
              <dl className="grid grid-cols-[6rem_1fr] gap-x-3 gap-y-1">
                <dt className="text-muted-foreground">{t("list.size")}</dt>
                <dd className="tabular-nums">{formatBytes(probe.size)}</dd>
                <dt className="text-muted-foreground">{t("add.category")}</dt>
                <dd>{categoryLabel(probe.category)}</dd>
                <dt className="text-muted-foreground">{t("common.pause")}</dt>
                <dd>{probe.resumable ? t("add.canPause") : t("add.notPossible")}</dd>
              </dl>
            )}
          </div>

          {playlist && <PlaylistPicker url={url.trim()} playlist={playlist} ticked={ticked} onTicked={setTicked} />}

          {probe && !probe.resumable && (
            <p role="note" className="flex gap-2 rounded-lg border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
              <AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-600 dark:text-amber-400" aria-hidden />
              <span>
                <strong>{t("add.noResumeTitle")}</strong> {t("add.noResume")}
              </span>
            </p>
          )}

          {playlist ? (
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" className="size-4 accent-primary" checked={numbered} onChange={(e) => setNumbered(e.target.checked)} />
              {t("playlist.number")}
            </label>
          ) : (
            <div className="grid gap-2">
              <Label htmlFor="add-name">{t("add.fileName")}</Label>
              <Input id="add-name" value={name} onChange={(e) => setName(e.target.value)} disabled={!probe} spellCheck={false} />
            </div>
          )}
          <div className="grid gap-2">
            <Label htmlFor="add-folder">{t("add.saveIn")}</Label>
            <div className="flex gap-2">
              <Input
                id="add-folder"
                value={folder}
                onChange={(e) => {
                  setFolder(e.target.value);
                  setFolderChosen(true);
                }}
                spellCheck={false}
              />
              <Button type="button" variant="outline" onClick={() => void choose()}>
                <FolderOpen aria-hidden /> {t("common.choose")}
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              {playlist ? t("playlist.folderNote", { name }) : t("add.noOverwrite")}
            </p>
          </div>

          {error && (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          )}

          <DialogFooter>
            {playlist ? (
              <Button type="submit" disabled={!ready}>
                {t("playlist.download", { count: ticked.size })}
              </Button>
            ) : video ? (
              <Button type="submit" disabled={!ready}>
                {t("add.download")}
              </Button>
            ) : (
              <>
                <Button type="button" variant="outline" disabled={!ready} onClick={() => void add(false)}>
                  {t("add.later")}
                </Button>
                <Button type="submit" disabled={!ready}>
                  {t("add.now")}
                </Button>
              </>
            )}
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
