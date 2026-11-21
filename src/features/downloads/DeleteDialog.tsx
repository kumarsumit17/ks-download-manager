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
import type { Download } from "./types";

interface Props {
  target: Download | null;
  onCancel: () => void;
  onConfirm: (d: Download, deleteFile: boolean) => void;
}

function explain(d: Download): string {
  if (d.status === "finished") return "It is removed from the list.";
  if (d.status === "downloading" && !d.resumable)
    return "The download stops and what it has downloaded so far is lost: this server does not support resuming.";
  if (d.status === "downloading") return "The download stops and its partial file is deleted.";
  return "Its partial file is deleted.";
}

export function DeleteDialog({ target, onCancel, onConfirm }: Props) {
  const [deleteFile, setDeleteFile] = useState(false);
  useEffect(() => setDeleteFile(false), [target?.id]);

  return (
    <AlertDialog open={target !== null} onOpenChange={(o) => !o && onCancel()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete “{target?.name}”?</AlertDialogTitle>
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
            Also delete the file from the disk
          </label>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={() => target && onConfirm(target, deleteFile)}>Delete</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
