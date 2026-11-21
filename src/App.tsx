import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { X } from "lucide-react";
import { Toolbar } from "@/components/layout/Toolbar";
import { Sidebar } from "@/components/layout/Sidebar";
import { StatusBar } from "@/components/layout/StatusBar";
import { TooltipProvider } from "@/components/ui/tooltip";
import { DownloadList, type RowActions } from "@/features/downloads/DownloadList";
import { DownloadDetails } from "@/features/downloads/DownloadDetails";
import { AddDownloadDialog } from "@/features/downloads/AddDownloadDialog";
import { DeleteDialog } from "@/features/downloads/DeleteDialog";
import { QueuesDialog } from "@/features/queues/QueuesDialog";
import { SettingsDialog } from "@/features/settings/SettingsDialog";
import { t } from "@/lib/text";
import { api, message, onQuitRequested } from "@/features/downloads/api";
import { useDownloads, useQueues } from "@/features/downloads/store";
import {
  type Active,
  CANNOT_PAUSE,
  type Download,
  type Filter,
  canPause,
  canResume,
  cannotPause,
  matches,
} from "@/features/downloads/types";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";

const plural = (n: number, one: string, many: string) => (n === 1 ? `1 ${one}` : `${n} ${many}`);

/** True while typing in a field, where Space and Delete must keep their usual meaning. */
function typing(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  return !!t && (t.isContentEditable || ["INPUT", "TEXTAREA", "SELECT", "BUTTON"].includes(t.tagName));
}

export default function App() {
  const { downloads, loaded } = useDownloads();
  const queues = useQueues();
  const [queuesOpen, setQueuesOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [adding, setAdding] = useState(false);
  const [deleting, setDeleting] = useState<Download | null>(null);
  const [quitting, setQuitting] = useState<Active | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>({ kind: "all" });
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return downloads.filter((d) => matches(d, filter) && (q === "" || d.name.toLowerCase().includes(q)));
  }, [downloads, filter, query]);

  const selected = visible.find((d) => d.id === selectedId) ?? null;
  const active = downloads.filter((d) => d.status === "downloading");
  const unfinished = downloads.filter((d) => d.status !== "finished").length;

  // Every command reports failure as a message; show it instead of failing silently.
  const run = useCallback((p: Promise<unknown>) => {
    p.catch((e) => setNotice(message(e)));
  }, []);

  const actions: RowActions = useMemo(
    () => ({
      open: (d) => run(api.openFile(d.id)),
      openFolder: (d) => run(api.openFolder(d.id)),
      resume: (d) => run(api.resume(d.id)),
      pause: (d) => run(api.pause(d.id)),
      copyLink: (d) => run(navigator.clipboard.writeText(d.url)),
      moveToQueue: (d, queue) => run(api.moveToQueue(d.id, queue)),
      remove: (d) => setDeleting(d),
    }),
    [run],
  );

  const startQueue = useCallback((q: number | null) => run(api.startQueue(q)), [run]);
  const stopQueue = useCallback(
    (q: number | null) =>
      run(
        api.stopQueue(q).then((kept) => {
          if (kept > 0) setNotice(t("stopKept", { n: kept }));
        }),
      ),
    [run],
  );

  // A deleted queue's filter falls back to all downloads.
  useEffect(() => {
    if (filter.kind === "queue" && filter.queue !== null && !queues.some((q) => q.id === filter.queue)) setFilter({ kind: "all" });
  }, [filter, queues]);

  useEffect(() => {
    const off = onQuitRequested(setQuitting);
    return () => void off.then((f) => f());
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key.toLowerCase() === "n") {
        e.preventDefault();
        setAdding(true);
      } else if (mod && e.key.toLowerCase() === "f") {
        e.preventDefault();
        searchRef.current?.focus();
      } else if (document.querySelector("[role=dialog],[role=alertdialog]") || typing(e) || !selected) {
        return;
      } else if (e.key === " ") {
        e.preventDefault();
        if (canPause(selected)) actions.pause(selected);
        else if (canResume(selected)) actions.resume(selected);
        else if (cannotPause(selected)) setNotice(CANNOT_PAUSE);
      } else if (e.key === "Delete" || (e.key === "Backspace" && mod)) {
        e.preventDefault();
        setDeleting(selected);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected, actions]);

  return (
    <TooltipProvider delayDuration={400}>
      <div className="flex h-full flex-col bg-background text-foreground">
        <Toolbar
          canResume={selected !== null && canResume(selected)}
          canPause={selected !== null && canPause(selected)}
          pauseHint={selected && cannotPause(selected) ? CANNOT_PAUSE : undefined}
          canDelete={selected !== null}
          canClearAll={downloads.length > 0}
          onAdd={() => setAdding(true)}
          onResume={() => selected && actions.resume(selected)}
          onPause={() => selected && actions.pause(selected)}
          onDelete={() => selected && setDeleting(selected)}
          onClearAll={() => setConfirmClear(true)}
          onQueues={() => setQueuesOpen(true)}
          onSettings={() => setSettingsOpen(true)}
          query={query}
          onQuery={setQuery}
          searchRef={searchRef}
        />
        <div className="flex min-h-0 flex-1">
          <Sidebar
            downloads={downloads}
            queues={queues}
            selected={filter}
            onSelect={setFilter}
            onStartQueue={startQueue}
            onStopQueue={stopQueue}
          />
          <main className="flex min-w-0 flex-1 flex-col">
            {notice && (
              <div role="alert" className="flex items-start gap-2 border-b bg-destructive/10 px-3 py-2 text-sm text-destructive">
                <span className="flex-1">{notice}</span>
                <button type="button" aria-label="Dismiss" onClick={() => setNotice(null)} className="rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring">
                  <X className="size-4" aria-hidden />
                </button>
              </div>
            )}
            <DownloadList downloads={visible} selectedId={selected?.id ?? null} onSelect={setSelectedId} actions={actions} queues={queues} loaded={loaded} />
            {selected && <DownloadDetails d={selected} />}
          </main>
        </div>
        <StatusBar downloads={downloads} />

        <AddDownloadDialog open={adding} onOpenChange={setAdding} onAdded={setSelectedId} />
        <QueuesDialog
          open={queuesOpen}
          onOpenChange={setQueuesOpen}
          queues={queues}
          downloads={downloads}
          onStart={startQueue}
          onStop={stopQueue}
        />
        <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />

        <DeleteDialog
          target={deleting}
          onCancel={() => setDeleting(null)}
          onConfirm={(d, deleteFile) => {
            setDeleting(null);
            run(api.remove(d.id, deleteFile));
          }}
        />

        <AlertDialog open={confirmClear} onOpenChange={setConfirmClear}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Clear all downloads?</AlertDialogTitle>
              <AlertDialogDescription>
                This removes {downloads.length === 1 ? "the 1 download" : `all ${downloads.length} downloads`} from the list
                {active.length > 0 ? ` and stops the ${active.length} still in progress` : ""}. Finished files on your disk are
                kept{unfinished > 0 ? "; partial files of unfinished downloads are deleted" : ""}.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction
                onClick={() => {
                  setSelectedId(null);
                  run(api.clearAll());
                }}
              >
                Clear all
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>

        <AlertDialog open={quitting !== null} onOpenChange={(o) => !o && setQuitting(null)}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Quit while downloading?</AlertDialogTitle>
              <AlertDialogDescription>
                {quitting && quitting.running - quitting.notResumable > 0 &&
                  `${plural(quitting.running - quitting.notResumable, "download is", "downloads are")} paused and can be resumed next time. `}
                {quitting && quitting.notResumable > 0 && (
                  <strong>
                    {plural(quitting.notResumable, "download", "downloads")} cannot be resumed and will be lost (the server
                    does not support resuming).
                  </strong>
                )}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Keep downloading</AlertDialogCancel>
              <AlertDialogAction onClick={() => run(api.quit())}>Quit</AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </TooltipProvider>
  );
}
