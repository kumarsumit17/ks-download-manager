import { useEffect, useState } from "react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { t } from "@/lib/i18n";
import { api } from "@/features/downloads/api";

/**
 * Shown once, on the first start (specs/video-playlists.md, "Limits"): the user is
 * responsible for what they download, and the app does not bypass copy protection. Only
 * "I understand" closes it, and the choice is saved in settings.json (`noticeSeen`).
 */
export function FirstRunNotice() {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    api.settings().then((s) => !s.noticeSeen && setOpen(true), () => {});
  }, []);

  const accept = () => {
    setOpen(false);
    // If it cannot be saved, the notice simply shows again next time.
    api.acceptNotice().catch(() => {});
  };

  return (
    <AlertDialog open={open}>
      <AlertDialogContent onEscapeKeyDown={(e) => e.preventDefault()}>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("notice.title")}</AlertDialogTitle>
          <AlertDialogDescription asChild>
            <div className="grid gap-2">
              <p>{t("notice.rights")}</p>
              <p>{t("notice.drm")}</p>
              <p>{t("notice.free")}</p>
            </div>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogAction autoFocus onClick={accept}>
            {t("notice.accept")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
