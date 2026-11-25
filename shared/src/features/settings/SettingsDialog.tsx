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
import { setLanguage, t, type TextKey } from "@/lib/i18n";
import { api, message, type LeftoverItem, type Settings } from "@/features/downloads/api";
import { formatBytes } from "@/features/downloads/format";
import type { Category } from "@/features/downloads/types";
import { isDevVersion } from "ks-utility";
import { checkNow, installNow, noteText, useUpdates } from "@/features/update/appUpdate";

const categories: { key: Category; label: TextKey }[] = [
  { key: "video", label: "category.video" },
  { key: "audio", label: "category.audio" },
  { key: "images", label: "category.images" },
  { key: "other", label: "category.other" },
];

const languages: { value: string; label: TextKey }[] = [
  { value: "system", label: "common.language.system" },
  { value: "en", label: "common.language.english" },
  { value: "hi", label: "common.language.hindi" },
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
  const appUpdate = useUpdates();
  const [leftovers, setLeftovers] = useState<LeftoverItem[] | null>(null);
  const [leftoverNote, setLeftoverNote] = useState<string | null>(null);
  // Paths ticked for removal; every item found starts ticked.
  const [ticked, setTicked] = useState<Set<string>>(new Set());
  const [confirming, setConfirming] = useState(false);

  const look = (rescan: boolean) => {
    setLeftovers(null);
    api.leftovers(rescan).then(
      (items) => {
        setLeftovers(items);
        setTicked(new Set(items.map((l) => l.path)));
      },
      (e) => setLeftoverNote(message(e)),
    );
  };

  const tick = (path: string, on: boolean) =>
    setTicked((old) => {
      const next = new Set(old);
      if (on) next.add(path);
      else next.delete(path);
      return next;
    });

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
      // Redraws the window in the chosen language (after the dialog has closed).
      if (saved.language !== s.language) setLanguage(saved.language);
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
  const chosen = leftovers?.filter((l) => ticked.has(l.path)) ?? [];
  const chosenSize = chosen.reduce((a, l) => a + l.size, 0);

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>{t("common.settings")}</DialogTitle>
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
              <Section title={t("settings.language")}>
                <div className="grid grid-cols-[1fr_12rem] items-center gap-3">
                  <Label htmlFor="set-language">{t("common.language")}</Label>
                  <select
                    id="set-language"
                    value={s.language}
                    onChange={(e) => setS({ ...s, language: e.target.value })}
                    className="h-8 rounded-md border bg-background px-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    {languages.map((l) => (
                      <option key={l.value} value={l.value}>
                        {t(l.label)}
                      </option>
                    ))}
                  </select>
                </div>
              </Section>

              <Separator />

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
                      <FolderOpen aria-hidden /> {t("common.choose")}
                    </Button>
                  </div>
                ))}
              </Section>

              <Separator />

              <Section title={t("add.title")}>
                <label className="flex items-start gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={s.clipboardPrefill}
                    onChange={(e) => setS({ ...s, clipboardPrefill: e.target.checked })}
                    className="mt-0.5 size-4 accent-primary"
                  />
                  <span>
                    {t("settings.clipboard")}
                    <span className="block text-xs text-muted-foreground">{t("settings.clipboardHint")}</span>
                  </span>
                </label>
              </Section>

              <Separator />

              <Section title={t("settings.computer")}>
                <label className="flex items-start gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={s.keepAwake}
                    onChange={(e) => setS({ ...s, keepAwake: e.target.checked })}
                    className="mt-0.5 size-4 accent-primary"
                  />
                  <span>
                    {t("settings.keepAwake")}
                    <span className="block text-xs text-muted-foreground">{t("settings.keepAwakeHint")}</span>
                  </span>
                </label>
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

              <Section title={t("common.appUpdate.section")}>
                <p className="text-xs text-muted-foreground">
                  {appUpdate.current === null ? null : isDevVersion(appUpdate.current) ? t("common.appUpdate.dev") : t("common.appUpdate.current", { version: appUpdate.current })}
                </p>
                <label className="flex items-start gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={s.updateCheck}
                    onChange={(e) => setS({ ...s, updateCheck: e.target.checked })}
                    className="mt-0.5 size-4 accent-primary"
                  />
                  <span>
                    {t("common.appUpdate.auto")}
                    <span className="block text-xs text-muted-foreground">{t("common.appUpdate.autoHint")}</span>
                  </span>
                </label>
                <div className="flex flex-wrap items-center gap-3">
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={appUpdate.phase === "checking" || appUpdate.phase === "downloading"}
                    onClick={() => void checkNow()}
                  >
                    {appUpdate.phase === "checking" && <Loader2 className="animate-spin motion-reduce:animate-none" aria-hidden />}
                    {t("common.appUpdate.checkButton")}
                  </Button>
                  {appUpdate.available && appUpdate.phase !== "opened" && (
                    <Button type="button" size="sm" disabled={appUpdate.phase === "downloading" || appUpdate.phase === "checking"} onClick={() => void installNow()}>
                      {appUpdate.phase === "downloading" && <Loader2 className="animate-spin motion-reduce:animate-none" aria-hidden />}
                      {t("common.appUpdate.installButton", { size: formatBytes(appUpdate.available.size ?? null) })}
                    </Button>
                  )}
                  <span aria-live="polite" className="text-xs text-muted-foreground">
                    {noteText(appUpdate.note)}
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
                      : t("leftoversFound", { count: leftovers.length, size: formatBytes(total) })}
                </div>
                {leftovers && leftovers.length > 0 && (
                  <ul className="max-h-32 overflow-y-auto rounded-md border p-2 text-xs">
                    {leftovers.map((l) => (
                      <li key={l.path}>
                        <label className="flex items-center gap-2 py-0.5">
                          <input
                            type="checkbox"
                            checked={ticked.has(l.path)}
                            onChange={(e) => tick(l.path, e.target.checked)}
                            className="size-3.5 shrink-0 accent-primary"
                          />
                          <span className="flex-1 truncate" title={l.path}>
                            {l.path}
                          </span>
                          <span className="tabular-nums text-muted-foreground">{formatBytes(l.size)}</span>
                        </label>
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
                    disabled={chosen.length === 0}
                    onClick={() => setConfirming(true)}
                  >
                    {t("common.remove")}
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
                  {t("common.cancel")}
                </Button>
                <Button type="submit" disabled={saving}>
                  {t("common.save")}
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
            <AlertDialogDescription>{t("removeLeftoversText", { count: chosen.length, size: formatBytes(chosenSize) })}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                const paths = chosen.map((l) => l.path);
                setLeftovers(null);
                api.removeLeftovers(paths).then(
                  ([removed, kept]) => {
                    setLeftoverNote(t("removedResult", { removed, kept }));
                    look(false);
                  },
                  (e) => setLeftoverNote(message(e)),
                );
              }}
            >
              {t("common.remove")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
