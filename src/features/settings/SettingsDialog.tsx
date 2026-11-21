import { useEffect, useState, type ReactNode } from "react";
import { FolderOpen, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
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
import { t, type TextKey } from "@/lib/text";
import { api, message, type LeftoverItem, type Settings } from "@/features/downloads/api";
import { formatBytes } from "@/features/downloads/format";
import type { Category } from "@/features/downloads/types";

const categories: { key: Category; label: TextKey }[] = [
  { key: "video", label: "categoryVideo" },
  { key: "audio", label: "categoryAudio" },
  { key: "images", label: "categoryImages" },
  { key: "other", label: "categoryOther" },
];

/** Settings keep the speed limit in bytes per second; the field shows KB/s. */
const KB = 1024;

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="grid gap-3">
      <h3 className="text-sm font-semibold">{title}</h3>
      {children}
    </section>
  );
}

function NumberField(p: { id: string; label: string; value: number; min: number; max: number; onChange: (n: number) => void; hint?: string }) {
  return (
    <div className="grid grid-cols-[1fr_6rem] items-center gap-x-3 gap-y-1">
      <Label htmlFor={p.id}>{p.label}</Label>
      <Input
        id={p.id}
        type="number"
        inputMode="numeric"
        min={p.min}
        max={p.max}
        value={Number.isFinite(p.value) ? p.value : ""}
        onChange={(e) => p.onChange(e.target.valueAsNumber)}
        className="h-8 text-right tabular-nums"
      />
      {p.hint && <p className="col-span-2 text-xs text-muted-foreground">{p.hint}</p>}
    </div>
  );
}

export function SettingsDialog({ open, onOpenChange }: Props) {
  const [s, setS] = useState<Settings | null>(null);
  const [kbps, setKbps] = useState(0);
  const [limited, setLimited] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [update, setUpdate] = useState<{ busy: boolean; note: string | null }>({ busy: false, note: null });
  const [leftovers, setLeftovers] = useState<LeftoverItem[] | null>(null);
  const [leftoverNote, setLeftoverNote] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);

  const look = (rescan: boolean) => {
    setLeftovers(null);
    api.leftovers(rescan).then(setLeftovers, (e) => setLeftoverNote(message(e)));
  };

  useEffect(() => {
    if (!open) return;
    setError(null);
    setUpdate({ busy: false, note: null });
    setLeftoverNote(null);
    api.settings().then(
      (v) => {
        setS(v);
        setLimited(v.speedLimit !== null);
        setKbps(v.speedLimit === null ? 1024 : Math.max(1, Math.round(v.speedLimit / KB)));
      },
      (e) => setError(message(e)),
    );
    look(false);
  }, [open]);

  const save = async () => {
    if (!s) return;
    setSaving(true);
    setError(null);
    try {
      const saved = await api.saveSettings({ ...s, speedLimit: limited && kbps > 0 ? Math.round(kbps * KB) : null });
      setS(saved);
      onOpenChange(false);
    } catch (e) {
      setError(message(e));
    } finally {
      setSaving(false);
    }
  };

  const choose = async (c: Category) => {
    if (!s) return;
    try {
      const start = s.folders[c] ?? (await api.categoryFolder(c));
      const picked = await api.pickFolder(t("chooseFolder"), start);
      if (picked) setS({ ...s, folders: { ...s.folders, [c]: picked } });
    } catch (e) {
      setError(message(e));
    }
  };

  const updateVideo = async () => {
    setUpdate({ busy: true, note: t("updating") });
    try {
      const r = await api.updateVideoComponent();
      setUpdate({ busy: false, note: r.result === "updated" ? t("updated", { from: r.from, to: r.to }) : t("upToDate", { version: r.version }) });
    } catch (e) {
      setUpdate({ busy: false, note: message(e) });
    }
  };

  const total = leftovers?.reduce((a, l) => a + l.size, 0) ?? 0;

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>{t("settings")}</DialogTitle>
            <DialogDescription>{t("settingsDescription")}</DialogDescription>
          </DialogHeader>

          {s && (
            <form
              className="grid gap-5"
              onSubmit={(e) => {
                e.preventDefault();
                void save();
              }}
            >
              <Section title={t("limits")}>
                <NumberField id="set-active" label={t("maxActive")} value={s.maxActive} min={1} max={20} onChange={(n) => setS({ ...s, maxActive: n })} />
                <NumberField
                  id="set-total"
                  label={t("totalConnections")}
                  value={s.totalConnections}
                  min={1}
                  max={256}
                  onChange={(n) => setS({ ...s, totalConnections: n })}
                />
                <NumberField
                  id="set-per"
                  label={t("perDownload")}
                  value={s.perDownload}
                  min={1}
                  max={32}
                  onChange={(n) => setS({ ...s, perDownload: n })}
                  hint={t("perDownloadHint")}
                />
                <div className="grid grid-cols-[1fr_auto_6rem_auto] items-center gap-x-3 gap-y-1">
                  <span className="text-sm font-medium">{t("speedLimit")}</span>
                  <label className="flex items-center gap-2 text-sm">
                    <input type="checkbox" checked={!limited} onChange={(e) => setLimited(!e.target.checked)} className="size-4 accent-primary" />
                    {t("noLimit")}
                  </label>
                  <Input
                    aria-label={`${t("speedLimit")} (KB/s)`}
                    type="number"
                    min={1}
                    disabled={!limited}
                    value={Number.isFinite(kbps) ? kbps : ""}
                    onChange={(e) => setKbps(e.target.valueAsNumber)}
                    className="h-8 text-right tabular-nums"
                  />
                  <span className="text-sm text-muted-foreground">KB/s</span>
                  <p className="col-span-4 text-xs text-muted-foreground">{t("speedLimitHint")}</p>
                </div>
              </Section>

              <Separator />

              <Section title={t("folders")}>
                {categories.map((c) => (
                  <div key={c.key} className="grid grid-cols-[5rem_1fr_auto] items-center gap-2">
                    <Label htmlFor={`set-folder-${c.key}`}>{t(c.label)}</Label>
                    <Input
                      id={`set-folder-${c.key}`}
                      value={s.folders[c.key] ?? ""}
                      placeholder={t("folderDefault")}
                      spellCheck={false}
                      onChange={(e) => setS({ ...s, folders: { ...s.folders, [c.key]: e.target.value || null } })}
                      className="h-8"
                    />
                    <Button type="button" size="sm" variant="outline" onClick={() => void choose(c.key)}>
                      <FolderOpen aria-hidden /> {t("choose")}
                    </Button>
                  </div>
                ))}
              </Section>

              <Separator />

              <Section title={t("video")}>
                <label className="flex items-start gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={s.preferMp4}
                    onChange={(e) => setS({ ...s, preferMp4: e.target.checked })}
                    className="mt-0.5 size-4 accent-primary"
                  />
                  <span>
                    {t("preferMp4")}
                    <span className="block text-xs text-muted-foreground">{t("preferMp4Hint")}</span>
                  </span>
                </label>
                <div className="flex items-center gap-3">
                  <Button type="button" size="sm" variant="outline" disabled={update.busy} onClick={() => void updateVideo()}>
                    {update.busy && <Loader2 className="animate-spin motion-reduce:animate-none" aria-hidden />}
                    {t("updateVideo")}
                  </Button>
                  <span aria-live="polite" className="text-xs text-muted-foreground">
                    {update.note}
                  </span>
                </div>
              </Section>

              <Separator />

              <Section title={t("leftovers")}>
                <p className="text-xs text-muted-foreground">{t("leftoversHint")}</p>
                <div aria-live="polite" className="text-sm">
                  {leftovers === null
                    ? t("leftoversLooking")
                    : leftovers.length === 0
                      ? t("leftoversNone")
                      : t("leftoversFound", { n: leftovers.length, size: formatBytes(total) })}
                </div>
                {leftovers && leftovers.length > 0 && (
                  <ul className="max-h-32 overflow-y-auto rounded-md border p-2 text-xs">
                    {leftovers.map((l) => (
                      <li key={l.path} className="flex gap-2 py-0.5">
                        <span className="flex-1 truncate" title={l.path}>
                          {l.path}
                        </span>
                        <span className="tabular-nums text-muted-foreground">{formatBytes(l.size)}</span>
                      </li>
                    ))}
                  </ul>
                )}
                <div className="flex items-center gap-2">
                  <Button type="button" size="sm" variant="outline" disabled={leftovers === null} onClick={() => look(true)}>
                    {t("lookAgain")}
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="destructive"
                    disabled={!leftovers || leftovers.length === 0}
                    onClick={() => setConfirming(true)}
                  >
                    {t("remove")}
                  </Button>
                  {leftoverNote && <span className="text-xs text-muted-foreground">{leftoverNote}</span>}
                </div>
              </Section>

              {error && (
                <p role="alert" className="text-sm text-destructive">
                  {error}
                </p>
              )}

              <DialogFooter>
                <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
                  {t("cancel")}
                </Button>
                <Button type="submit" disabled={saving}>
                  {t("save")}
                </Button>
              </DialogFooter>
            </form>
          )}
          {!s && error && (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          )}
        </DialogContent>
      </Dialog>

      <AlertDialog open={confirming} onOpenChange={setConfirming}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("removeLeftoversTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{t("removeLeftoversText", { n: leftovers?.length ?? 0, size: formatBytes(total) })}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setLeftovers(null);
                api.removeLeftovers().then(
                  ([removed, kept]) => {
                    setLeftoverNote(t("removedResult", { removed, kept }));
                    look(false);
                  },
                  (e) => setLeftoverNote(message(e)),
                );
              }}
            >
              {t("remove")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
