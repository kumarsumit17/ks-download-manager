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
import { t } from "@/lib/text";
import { api, message } from "./api";
import { formatBytes } from "./format";
import type { Category, ProbeView } from "./types";

const categoryLabel: Record<Category, string> = { video: "Video", audio: "Audio", images: "Images", other: "Other" };

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
      const r = await api.add(url.trim(), name, folder, start);
      onAdded(r.id);
      onOpenChange(false);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };

  const ready = probe !== null && name.trim() !== "" && folder.trim() !== "" && !busy;
  const video = probe?.kind === "video";

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Add download</DialogTitle>
          <DialogDescription>Paste a link to a file or a video page. Its details appear before it starts.</DialogDescription>
        </DialogHeader>
        <form
          className="grid gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (ready) void add(true);
          }}
        >
          <div className="grid gap-2">
            <Label htmlFor="add-url">Link</Label>
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
                <Loader2 className="size-4 animate-spin motion-reduce:animate-none" aria-hidden /> Checking the link…
              </span>
            )}
            {probe && video && (
              <dl className="grid grid-cols-[6rem_1fr] gap-x-3 gap-y-1">
                <dt className="text-muted-foreground">Video</dt>
                <dd className="flex items-center gap-1.5">
                  <Film className="size-4 shrink-0" aria-hidden /> Best available quality
                </dd>
                <dt className="text-muted-foreground">Size</dt>
                <dd className="tabular-nums">{probe.size === null ? "Unknown" : `about ${formatBytes(probe.size)}`}</dd>
              </dl>
            )}
            {probe && !video && (
              <dl className="grid grid-cols-[6rem_1fr] gap-x-3 gap-y-1">
                <dt className="text-muted-foreground">Size</dt>
                <dd className="tabular-nums">{formatBytes(probe.size)}</dd>
                <dt className="text-muted-foreground">Category</dt>
                <dd>{categoryLabel[probe.category]}</dd>
                <dt className="text-muted-foreground">Pause</dt>
                <dd>{probe.resumable ? "Can be paused and resumed" : "Not possible"}</dd>
              </dl>
            )}
          </div>

          {probe && !probe.resumable && (
            <p role="note" className="flex gap-2 rounded-lg border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
              <AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-600 dark:text-amber-400" aria-hidden />
              <span>
                This server does not support resuming. This download <strong>cannot be paused or resumed</strong>:
                stopping it, quitting the app or losing the connection for good means starting over.
              </span>
            </p>
          )}

          <div className="grid gap-2">
            <Label htmlFor="add-name">File name</Label>
            <Input id="add-name" value={name} onChange={(e) => setName(e.target.value)} disabled={!probe} spellCheck={false} />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="add-folder">Save in folder</Label>
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
                <FolderOpen aria-hidden /> {t("choose")}
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              If a file with this name is already there, the new one is saved as “name (1)”. Nothing is overwritten.
            </p>
          </div>

          {error && (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          )}

          <DialogFooter>
            {video ? (
              <Button type="submit" disabled={!ready}>
                Download
              </Button>
            ) : (
              <>
                <Button type="button" variant="outline" disabled={!ready} onClick={() => void add(false)}>
                  Download later
                </Button>
                <Button type="submit" disabled={!ready}>
                  Download now
                </Button>
              </>
            )}
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
