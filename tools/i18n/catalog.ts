// Builds the app's String Catalog from the web's dictionaries (see web.ts). The catalog's keys are the web's English
// strings in the form Swift looks them up: each `{name}` becomes the format specifier Swift's string interpolation
// writes (`%lld` for the count `n`, `%@` for anything else), and a `%` is doubled when the string has arguments.
// Values are the web's translations, converted the same way, with positional specifiers so a translation may
// reorder its arguments. The web's plural objects become String Catalog plural variations.
import { Entry, loadWeb, LOCALES, norm, Plural, WebStrings } from "./web.ts";

const CATEGORIES = ["zero", "one", "two", "few", "many", "other"];

/** The placeholder names of a web string, one per `{name}` occurrence (each is one Swift interpolation). */
export const argNames = (s: string): string[] => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]);
/** `n` is the count (an Int in Swift); every other placeholder is a String. `JS.t` in Swift builds the same key. */
export const specOf = (name: string): string => (name === "n" ? "lld" : "@");

/** The catalog key of a web string, as Swift's string interpolation spells it. A `tn()` text that never shows its
 * count (`none needed: small hole left for bridging`) still varies by it: `hidden` adds the count as a last,
 * unused `%lld`, which Swift's `JS.tn` appends to the key the same way. */
export function catalogKey(web: string, hidden = false): string {
  const args = argNames(web);
  if (args.length === 0 && !hidden) return web;
  const esc = (t: string) => t.replace(/%/g, "%%");
  const parts = web.split(/\{\w+\}/);
  return parts.map((p, i) => esc(p) + (i < args.length ? "%" + specOf(args[i]) : "")).join("") + (hidden ? "%lld" : "");
}

/** A translation as a format string over the key's arguments (`firstPlaceholder` = `%arg` for the plural count). */
function value(text: string, args: string[], plural: boolean, hidden = false): { v: string; stray: string[] } {
  const stray: string[] = [];
  const parts = text.split(/(\{\w+\})/);
  const out = parts.map((p) => {
    const m = p.match(/^\{(\w+)\}$/);
    if (!m) return args.length ? p.replace(/%/g, "%%") : p;
    const i = args.indexOf(m[1]);
    if (i < 0) { stray.push(m[1]); return p; }
    if (plural && m[1] === "n") return "%arg";
    return `%${i + 1}$${specOf(m[1])}`;
  });
  // a plural variation must show its count, and a hidden one never does: it goes between two U+2063 marks that
  // `JS.tn` strips from the result
  if (hidden) out.push(`${HIDDEN}${plural ? "%arg" : "%" + args.length + "$lld"}${HIDDEN}`);
  return { v: out.join(""), stray };
}

/** Brackets the hidden count in a translation (see `value`). */
const HIDDEN = "\u2063";

const unit = (v: string) => ({ stringUnit: { state: "translated", value: v } });

export interface Report {
  keys: number;
  strays: string[];
  normCollisions: string[];
  stringOnlyPairs: string[];
  missingN: string[];
}

type Localization = Record<string, unknown>;

/** The plural forms the web would pick for `entry` (`val[rule] || val.other || val.one`) as variation units. */
function pluralVariations(forms: Plural, args: string[], rep: Report, key: string, hidden: boolean): Record<string, unknown> {
  const other = forms.other ?? forms.one;
  const vars: Record<string, unknown> = {};
  for (const c of CATEGORIES) {
    const text = c === "other" ? other : forms[c];
    if (text === undefined || text === "") continue;
    const { v, stray } = value(text, args, true, hidden);
    stray.forEach((s) => rep.strays.push(`${key}: {${s}}`));
    vars[c] = unit(v);
  }
  return vars;
}

function localization(forms: Plural | string, key: string, rep: Report, hidden: boolean): Localization {
  const args = argNames(key);
  if (hidden) args.push("n");
  const nIdx = args.indexOf("n");
  if (typeof forms === "string") {
    const { v, stray } = value(forms, args, false);
    stray.forEach((s) => rep.strays.push(`${key}: {${s}}`));
    return unit(v);
  }
  if (nIdx < 0) { rep.missingN.push(key); const { v } = value(forms.other ?? forms.one, args, false); return unit(v); }
  if (args.length === 1) {
    // %lld is the only argument: variations at the top level, `%lld` in each form
    const vars: Record<string, unknown> = {};
    const other = forms.other ?? forms.one;
    for (const c of CATEGORIES) {
      const text = c === "other" ? other : forms[c];
      if (!text) continue;
      const { v, stray } = value(text, args, false, hidden);
      stray.forEach((s) => rep.strays.push(`${key}: {${s}}`));
      vars[c] = unit(v);
    }
    return { variations: { plural: vars } };
  }
  // several arguments: the sentence is a substitution `%#@n@` over the count, `%arg` inside each form
  const vars = pluralVariations(forms, args, rep, key, hidden);
  return {
    stringUnit: { state: "translated", value: "%#@n@" },
    substitutions: { n: { argNum: nIdx + 1, formatSpecifier: "lld", variations: { plural: vars } } },
  };
}

export async function buildCatalog(web?: WebStrings, aliases: Record<string, string> = {}): Promise<{ catalog: unknown; report: Report }> {
  web ??= await loadWeb();
  const rep: Report = { keys: 0, strays: [], normCollisions: [], stringOnlyPairs: [], missingN: [] };
  const pairOf = new Map<string, [string, string]>();
  for (const [s, p] of web.pairs) { pairOf.set(s, [s, p]); pairOf.set(p, [s, p]); }

  // the web matches a dictionary key through norm(): the last of several keys that norm alike wins
  const byNorm: Record<string, Map<string, Entry>> = {};
  for (const [lang, d] of Object.entries(web.dicts)) {
    byNorm[lang] = new Map();
    const seen = new Map<string, string>();
    for (const [k, v] of Object.entries(d)) {
      const nk = norm(k);
      if (seen.has(nk) && JSON.stringify(d[seen.get(nk)!]) !== JSON.stringify(v)) rep.normCollisions.push(`${lang}: ${k}`);
      seen.set(nk, k);
      byNorm[lang].set(nk, v);
    }
  }
  const lookup = (lang: string, k: string): Entry | undefined => byNorm[lang].get(norm(k));

  const keys = new Set<string>();
  for (const d of Object.values(web.dicts)) for (const k of Object.keys(d)) keys.add(k);
  for (const [s, p] of web.pairs) { keys.add(s); keys.add(p); }
  for (const a of Object.keys(aliases)) keys.add(a);

  const strings: Record<string, unknown> = {};
  for (const K of [...keys].sort()) {
    const src = aliases[K] ?? K;
    const pair = pairOf.get(src);
    const hidden = !!pair && !argNames(K).includes("n");
    const locs: Record<string, Localization> = {};
    for (const lang of Object.keys(LOCALES)) {
      let e: Entry | undefined;
      if (pair) {
        const [s, p] = pair;
        e = lookup(lang, p) ?? lookup(lang, s);
        if (typeof e === "string") {
          // tn() on a string entry: the web picks `singular` for 1 and `plural` otherwise, by the English rule
          const es = lookup(lang, s), ep = lookup(lang, p);
          if (typeof es === "string" && typeof ep === "string" && es !== ep) {
            e = { one: es, other: ep };
            rep.stringOnlyPairs.push(`${lang}: ${p}`);
          }
        }
      } else e = lookup(lang, src);
      if (e === undefined) continue;
      locs[LOCALES[lang]] = localization(e, K, rep, hidden);
    }
    if (pair) {
      const [s, p] = pair;
      // English plural forms: tn() picks `singular` for n == 1, `plural` otherwise
      locs["en"] = localization({ one: s, other: p }, K, rep, hidden);
    }
    if (Object.keys(locs).length === 0) continue;
    strings[catalogKey(K, hidden)] = { extractionState: "manual", localizations: locs };
  }
  rep.keys = Object.keys(strings).length;
  return { catalog: { sourceLanguage: "en", strings, version: "1.0" }, report: rep };
}

/** Xcode's JSON layout (`"key" : value`, two-space indent), so editing the file in Xcode makes no spurious diff. */
export function serialize(catalog: unknown): string {
  return JSON.stringify(catalog, null, 2).replace(/^(\s*"(?:[^"\\]|\\.)*"): /gm, "$1 : ") + "\n";
}
