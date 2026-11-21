// Sizes, speeds and times left come from ks-utility, formatted for the interface
// language with the built-in Intl (Latin digits).
import { formatBytes as bytes, formatSpeed as speed, formatTimeLeft as timeLeft } from "ks-utility";
import { currentLanguage, t } from "@/lib/i18n";

const options = () => ({ locale: currentLanguage(), unknown: t("common.unknown") });

export const formatBytes = (n: number | null) => bytes(n, options());
/** Empty when not moving. */
export const formatSpeed = (bps: number) => speed(bps, options());
/** "0 B/s" instead of nothing, for places that always show a speed. */
export const formatSpeedOrZero = (bps: number) => speed(bps, options()) || t("speed.zero");
export const formatTimeLeft = (seconds: number | null) => timeLeft(seconds, options());

export function formatDate(ms: number): string {
  return new Date(ms).toLocaleString(currentLanguage(), { dateStyle: "medium", timeStyle: "short", numberingSystem: "latn" });
}
