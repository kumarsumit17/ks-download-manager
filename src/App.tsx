import { useMemo, useState } from "react";
import { Toolbar } from "@/components/layout/Toolbar";
import { Sidebar } from "@/components/layout/Sidebar";
import { StatusBar } from "@/components/layout/StatusBar";
import { DownloadList } from "@/features/downloads/DownloadList";
import { sampleDownloads } from "@/features/downloads/sample";
import { type Filter, matches } from "@/features/downloads/types";
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

export default function App() {
  const [downloads, setDownloads] = useState(sampleDownloads);
  const [confirmClear, setConfirmClear] = useState(false);
  const [filter, setFilter] = useState<Filter>({ kind: "all" });
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return downloads.filter((d) => matches(d, filter) && (q === "" || d.name.toLowerCase().includes(q)));
  }, [downloads, filter, query]);

  const selected = visible.find((d) => d.id === selectedId) ?? null;
  const active = downloads.filter((d) => d.status === "downloading").length;

  return (
    <div className="flex h-full flex-col bg-background text-foreground">
      <Toolbar
        canResume={selected !== null && ["paused", "failed", "queued"].includes(selected.status)}
        canPause={selected?.status === "downloading"}
        canDelete={selected !== null}
        canClearAll={downloads.length > 0}
        onClearAll={() => setConfirmClear(true)}
        query={query}
        onQuery={setQuery}
      />
      <div className="flex min-h-0 flex-1">
        <Sidebar downloads={downloads} selected={filter} onSelect={setFilter} />
        <main className="flex min-w-0 flex-1 flex-col">
          <DownloadList downloads={visible} selectedId={selected?.id ?? null} onSelect={setSelectedId} />
        </main>
      </div>
      <StatusBar downloads={downloads} />
      <AlertDialog open={confirmClear} onOpenChange={setConfirmClear}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Clear all downloads?</AlertDialogTitle>
            <AlertDialogDescription>
              This removes {downloads.length === 1 ? "the 1 download" : `all ${downloads.length} downloads`} from the list
              {active > 0 ? ` and stops the ${active} still in progress` : ""}. Files already saved on your disk are kept.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setDownloads([]);
                setSelectedId(null);
              }}
            >
              Clear all
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
