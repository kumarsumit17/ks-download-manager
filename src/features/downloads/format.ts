// Sizes, speeds and times left are generic: they come from ks-utility.
export { formatBytes, formatSpeed, formatTimeLeft } from "ks-utility";

export function formatDate(ms: number): string {
  return new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}
