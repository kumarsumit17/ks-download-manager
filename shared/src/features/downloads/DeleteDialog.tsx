import { useEffect, useState } from "react";
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
import type { Download } from "./types";

interface Props {
  target: Download | null;
  onCancel: () => void;
  onConfirm: (d: Download, deleteFile: boolean) => void;
}

function explain(d: Download): string {
  if (d.status === "finished") return t("delete.finished");
  if (d.status === "downloading" && !d.resumable) return t("delete.lost");
  if (d.status === "downloading") return t("delete.stops");
  return t("delete.partial");
}

export function DeleteDialog({ target, onCancel, onConfirm }: Props) {
  const [deleteFile, setDeleteFile] = useState(false);
  useEffect(() => setDeleteFile(false), [target?.id]);

  return (
    <AlertDialog open={target !== null} onOpenChange={(o) => !o && onCancel()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("delete.title", { name: target?.name ?? "" })}</AlertDialogTitle>
          <AlertDialogDescription>{target && explain(target)}</AlertDialogDescription>
        </AlertDialogHeader>
        {target?.status === "finished" && (
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={deleteFile}
              onChange={(e) => setDeleteFile(e.target.checked)}
              className="size-4 accent-primary"
            />
            {t("delete.alsoFile")}
          </label>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
          <AlertDialogAction onClick={() => target && onConfirm(target, deleteFile)}>{t("common.delete")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
