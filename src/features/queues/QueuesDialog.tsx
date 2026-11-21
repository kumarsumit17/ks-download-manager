import { useEffect, useState } from "react";
import { Pencil, Play, Square, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
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
import { t } from "@/lib/i18n";
import { api, message } from "@/features/downloads/api";
import type { Download, Queue } from "@/features/downloads/types";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  queues: Queue[];
  downloads: Download[];
  onStart: (queue: number | null) => void;
  onStop: (queue: number | null) => void;
}

export function QueuesDialog({ open, onOpenChange, queues, downloads, onStart, onStop }: Props) {
  const [name, setName] = useState("");
  const [editing, setEditing] = useState<{ id: number; name: string } | null>(null);
  const [deleting, setDeleting] = useState<Queue | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setName("");
    setEditing(null);
    setError(null);
  }, [open]);

  const attempt = (p: Promise<unknown>, then?: () => void) =>
    p.then(
      () => {
        setError(null);
        then?.();
      },
      (e) => setError(message(e)),
    );

  const count = (id: number | null) => downloads.filter((d) => (d.queue ?? null) === id).length;
  const rows: { id: number | null; name: string }[] = [{ id: null, name: t("mainQueue") }, ...queues];

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("queues")}</DialogTitle>
            <DialogDescription>{t("queuesDescription")}</DialogDescription>
          </DialogHeader>

          <ul className="grid gap-1" aria-label={t("queues")}>
            {rows.map((q) => (
              <li key={q.id ?? "main"} className="flex items-center gap-2 rounded-md border px-2 py-1.5">
                {editing && editing.id === q.id ? (
                  <form
                    className="flex flex-1 items-center gap-2"
                    onSubmit={(e) => {
                      e.preventDefault();
                      void attempt(api.renameQueue(editing.id, editing.name), () => setEditing(null));
                    }}
                  >
                    <Input
                      autoFocus
                      aria-label={t("queueName")}
                      value={editing.name}
                      maxLength={60}
                      onChange={(e) => setEditing({ ...editing, name: e.target.value })}
                      className="h-8"
                    />
                    <Button type="submit" size="sm">
                      {t("common.save")}
                    </Button>
                    <Button type="button" size="sm" variant="ghost" onClick={() => setEditing(null)}>
                      {t("common.cancel")}
                    </Button>
                  </form>
                ) : (
                  <>
                    <span className="flex-1 truncate text-sm">{q.name}</span>
                    <span className="text-xs tabular-nums text-muted-foreground">{t("downloadsCount", { count: count(q.id) })}</span>
                    <Button size="sm" variant="ghost" onClick={() => onStart(q.id)} aria-label={`${t("start")}: ${q.name}`}>
                      <Play aria-hidden /> {t("start")}
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => onStop(q.id)} aria-label={`${t("stop")}: ${q.name}`}>
                      <Square aria-hidden /> {t("stop")}
                    </Button>
                    {q.id !== null && (
                      <>
                        <Button
                          size="icon"
                          variant="ghost"
                          className="size-8"
                          aria-label={`${t("common.rename")}: ${q.name}`}
                          title={t("common.rename")}
                          onClick={() => setEditing({ id: q.id as number, name: q.name })}
                        >
                          <Pencil aria-hidden />
                        </Button>
                        <Button
                          size="icon"
                          variant="ghost"
                          className="size-8"
                          aria-label={`${t("common.delete")}: ${q.name}`}
                          title={t("common.delete")}
                          onClick={() => setDeleting({ id: q.id as number, name: q.name })}
                        >
                          <Trash2 aria-hidden />
                        </Button>
                      </>
                    )}
                  </>
                )}
              </li>
            ))}
          </ul>

          <form
            className="grid gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void attempt(api.createQueue(name), () => setName(""));
            }}
          >
            <Label htmlFor="new-queue">{t("newQueue")}</Label>
            <div className="flex gap-2">
              <Input id="new-queue" value={name} maxLength={60} onChange={(e) => setName(e.target.value)} placeholder={t("queueName")} />
              <Button type="submit" disabled={name.trim() === ""}>
                {t("create")}
              </Button>
            </div>
          </form>

          {error && (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          )}

          <DialogFooter>
            <Button variant="outline" onClick={() => onOpenChange(false)}>
              {t("common.close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog open={deleting !== null} onOpenChange={(o) => !o && setDeleting(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("deleteQueueTitle", { name: deleting?.name ?? "" })}</AlertDialogTitle>
            <AlertDialogDescription>{t("deleteQueueText")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (deleting) void attempt(api.deleteQueue(deleting.id));
                setDeleting(null);
              }}
            >
              {t("common.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
