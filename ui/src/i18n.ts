// The interface language: English, with Turkish as an option (chosen in the installer and in Settings).
// The source text is English; TR maps it to Turkish. `{0}`, `{1}` are filled from the extra arguments.
import { TR } from "./i18n-tr";

export type Lang = "en" | "tr";

function stored(): Lang {
  try {
    return localStorage.getItem("lang") === "tr" ? "tr" : "en";
  } catch {
    return "en";
  }
}

/** Read once at start, before any text is drawn; changing it reloads the page. */
export const lang: Lang = stored();

export function setLang(l: Lang) {
  try {
    localStorage.setItem("lang", l);
  } catch {
    /* private storage: the page still follows the setting it was given */
  }
  if (l !== lang) location.reload();
}

export function tx(s: string, ...a: unknown[]): string {
  const base = lang === "tr" ? TR[s] ?? s : s;
  return a.length ? base.replace(/\{(\d+)\}/g, (m, i) => (a[+i] === undefined ? m : String(a[+i]))) : base;
}

/** The product's name in the interface language. */
export const BRAND = lang === "tr" ? "Linnea Kesit" : "Linnea";
