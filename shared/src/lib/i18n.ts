// Interface text in English and Hindi (specs/ui.md, "Languages"): every string is a key
// of src/locales/*.json or of ks-utility's shared catalogue, looked up with ks-utility's
// translator. The language follows the system unless Settings chose one; changing it
// re-renders the window (see main.tsx).

import { createTranslator, resolveLanguage, type Language, type Params } from "ks-utility";
import commonEn from "ks-utility/lang/common/en.json";
import commonHi from "ks-utility/lang/common/hi.json";
import appEn from "@/locales/en.json";
import appHi from "@/locales/hi.json";

export type TextKey = keyof typeof appEn | keyof typeof commonEn;

/** A message from Rust: a key and its parameters (src-tauri/src/msg.rs). */
export interface Msg {
  code: string;
  params: Record<string, string>;
}

const layers = [
  { en: commonEn, hi: commonHi },
  { en: appEn, hi: appHi },
];

let language: Language = resolveLanguage(null, navigator.languages);
let translate = createTranslator(language, ...layers);

const listeners = new Set<(l: Language) => void>();

/** Applies a saved choice ("system", "en" or "hi") and tells the listeners. */
export function setLanguage(choice: unknown): Language {
  language = resolveLanguage(choice, navigator.languages);
  translate = createTranslator(language, ...layers);
  document.documentElement.lang = language;
  for (const f of listeners) f(language);
  return language;
}

/** Called after each language change; returns the function that stops it. */
export function onLanguage(f: (l: Language) => void): () => void {
  listeners.add(f);
  return () => listeners.delete(f);
}

export function currentLanguage(): Language {
  return language;
}

/** The text for `key` in the interface language, with placeholders filled in. */
export function t(key: TextKey, params?: Params): string {
  return translate(key, params);
}

export function isMsg(v: unknown): v is Msg {
  return typeof v === "object" && v !== null && typeof (v as Msg).code === "string";
}

/** A Rust message in the interface language. Unknown codes show as the code itself. */
export function msgText(m: Msg): string {
  return translate(m.code, m.params);
}
