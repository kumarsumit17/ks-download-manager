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
import { FirstRunNotice } from "@/features/notice/FirstRunNotice";
import { UpdateBanner } from "@/features/update/UpdateBanner";
import { startUpdates } from "@/features/update/appUpdate";
import { t } from "@/lib/i18n";
import { api, message, onQuitRequested } from "@/features/downloads/api";
import { useDownloads, useQueues } from "@/features/downloads/store";
import {
  type Active,
  cannotPauseText,
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
      resumePlaylist: (id) => run(api.resumePlaylist(id)),
      pausePlaylist: (id) => run(api.pausePlaylist(id)),
    }),
    [run],
  );

  const startQueue = useCallback((q: number | null) => run(api.startQueue(q)), [run]);
  const stopQueue = useCallback(
    (q: number | null) =>
      run(
        api.stopQueue(q).then((kept) => {
          if (kept > 0) setNotice(t("stopKept", { count: kept }));
        }),
      ),
    [run],
  );

  // A deleted queue's filter falls back to all downloads.
  useEffect(() => {
    if (filter.kind === "queue" && filter.queue !== null && !queues.some((q) => q.id === filter.queue)) setFilter({ kind: "all" });
  }, [filter, queues]);

  // App updates: the version, an update already found, and the start-up check's result.
  useEffect(startUpdates, []);

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
        else if (cannotPause(selected)) setNotice(cannotPauseText());
      } else if (e.key === "Delete" || (e.key === "Backspace" && mod)) {
        e.preventDefault();
        setDeleting(selected);
      } else if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
        // The row menu from the keyboard: opened at the selected row.
        e.preventDefault();
        const row = document.getElementById(`download-row-${selected.id}`);
        const r = row?.getBoundingClientRect();
        if (row && r) row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left + 24, clientY: r.bottom }));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected, actions]);

  return (
    <TooltipProvider delayDuration={400}>
      <FirstRunNotice />
      <div className="flex h-full flex-col bg-background text-foreground">
        <Toolbar
          canResume={selected !== null && canResume(selected)}
          canPause={selected !== null && canPause(selected)}
          pauseHint={selected && cannotPause(selected) ? cannotPauseText() : undefined}
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
            <UpdateBanner />
            {notice && (
              <div role="alert" className="flex items-start gap-2 border-b bg-destructive/10 px-3 py-2 text-sm text-destructive">
                <span className="flex-1">{notice}</span>
                <button type="button" aria-label={t("common.close")} onClick={() => setNotice(null)} className="rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring">
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
              <AlertDialogTitle>{t("clear.title")}</AlertDialogTitle>
              <AlertDialogDescription>
                {[
                  t("clear.removes", { count: downloads.length }),
                  active.length > 0 && t("clear.stops", { count: active.length }),
                  t("clear.keeps"),
                  unfinished > 0 && t("clear.partials"),
                ]
                  .filter(Boolean)
                  .join(" ")}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
              <AlertDialogAction
                onClick={() => {
                  setSelectedId(null);
                  run(api.clearAll());
                }}
              >
                {t("clear.confirm")}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>

        <AlertDialog open={quitting !== null} onOpenChange={(o) => !o && setQuitting(null)}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t("quit.title")}</AlertDialogTitle>
              <AlertDialogDescription>
                {quitting && quitting.running - quitting.notResumable > 0 &&
                  `${t("quit.paused", { count: quitting.running - quitting.notResumable })} `}
                {quitting && quitting.notResumable > 0 && <strong>{t("quit.lost", { count: quitting.notResumable })}</strong>}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{t("quit.keep")}</AlertDialogCancel>
              <AlertDialogAction onClick={() => run(api.quit())}>{t("quit.confirm")}</AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </TooltipProvider>
  );
}
