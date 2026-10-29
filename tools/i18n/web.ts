// Reads the web app's strings from the pinned JS checkout (UPSTREAM): the six dictionaries in web/i18n/, the
// singular/plural pairs the web hands to `tn()`, and every English string the web can show (JS literals and the
// index.html text, titles and placeholders). gen.ts builds the String Catalog from the first two; parity.ts uses the
// last to say whether a string in the app has a web key.
import * as acorn from "npm:acorn@8.14.0";
import { UPSTREAM_ROOT } from "../../parity/oracle/lib/pin.js";

export const WEB_ROOT = `${UPSTREAM_ROOT}/web`;

/** web language code -> String Catalog locale. English is the source language. */
export const LOCALES: Record<string, string> = { de: "de", zh: "zh-Hans", pt: "pt", ar: "ar", nl: "nl" };

export type Plural = Record<string, string>;
export type Entry = string | Plural;

/** `norm` of web/ui/i18n.js: the web matches a string to a dictionary key through this, so quote, dash and space
 * variants of one string are one key. Kept byte-for-byte (the first space class is U+00A0). */
export const norm = (s: string): string =>
  (s || "")
    .replace(/[“”"]/g, '"')
    .replace(/[‘’']/g, "'")
    .replace(/[—–—–]/g, "-")
    .replace(/ /g, " ")
    .replace(/\s+/g, " ")
    .trim();

/** The same string as `parity.ts` sees it in Swift source: `{name}` and `%lld`/`%@`/`%arg` are one `%arg`. */
export const argForm = (s: string): string => s.replace(/\{\w+\}/g, "%arg").replace(/%(?:\d+\$)?(?:lld|ld|d|@|lf|f|arg)/g, "%arg");

/** native-only.json: the strings the native app shows that the web has no key for (menus, undo names, sheets, errors),
 * each with the reason and its translations (`de`, `zh`, `pt`, `ar`, `nl`; a plural is an object of forms). `dev`
 * marks a debug-build string that is not translated. They join the web's dictionaries. */
export type NativeEntry = { why: string; dev?: boolean; like?: string } & Record<string, Entry | boolean | string | undefined>;
export function loadNative(): Record<string, NativeEntry> {
  return JSON.parse(Deno.readTextFileSync(new URL("native-only.json", import.meta.url)));
}

export async function loadDicts(): Promise<Record<string, Record<string, Entry>>> {
  const out: Record<string, Record<string, Entry>> = {};
  const native = loadNative();
  for (const lang of Object.keys(LOCALES)) {
    out[lang] = { ...(await import(`file://${WEB_ROOT}/i18n/${lang}.js`)).default };
    for (const [k, e] of Object.entries(native)) {
      // `like`: the same words in another case or punctuation as a web string, which brings its translations
      const likeKey = e.like === undefined ? undefined : Object.keys(out[lang]).find((x) => norm(x) === norm(e.like!));
      if (e.like !== undefined && Object.values(out).length === 0) { /* first language: nothing to check yet */ }
      const v = e[lang] ?? (likeKey === undefined ? undefined : out[lang][likeKey]);
      if (v === undefined || typeof v === "boolean") continue;
      if (k in out[lang]) throw new Error(`native-only.json: "${k}" is already a web key for ${lang}; remove it from native-only.json`);
      out[lang][k] = v as Entry;
    }
  }
  return out;
}

// ---- JS: literals and tn() pairs --------------------------------------------------------------------------------

// deno-lint-ignore no-explicit-any
type Node = any;

function fold(n: Node): string | null {
  if (!n) return null;
  if (n.type === "Literal") return typeof n.value === "string" ? n.value : null;
  if (n.type === "TemplateLiteral" && n.expressions.length === 0) return n.quasis.map((q: Node) => q.value.cooked).join("");
  if (n.type === "BinaryExpression" && n.operator === "+") {
    const a = fold(n.left), b = fold(n.right);
    return a !== null && b !== null ? a + b : null;
  }
  return null;
}

function walk(n: Node, visit: (n: Node) => void) {
  if (!n || typeof n !== "object") return;
  if (Array.isArray(n)) { for (const c of n) walk(c, visit); return; }
  if (typeof n.type === "string") visit(n);
  for (const k of Object.keys(n)) {
    if (k === "loc" || k === "start" || k === "end") continue;
    const v = n[k];
    if (v && typeof v === "object") walk(v, visit);
  }
}

export interface WebJs {
  /** every string literal in the web's UI code */
  literals: Set<string>;
  /** `tn(singular, plural, ...)` calls with literal arguments */
  pairs: Array<[string, string]>;
  /** `t('literal')` and `tn()` first arguments */
  tKeys: Set<string>;
}

export function scanJs(): WebJs {
  const files: string[] = [];
  for (const dir of [WEB_ROOT, `${WEB_ROOT}/ui`]) {
    for (const e of Deno.readDirSync(dir)) if (e.isFile && e.name.endsWith(".js")) files.push(`${dir}/${e.name}`);
  }
  files.sort();
  const literals = new Set<string>(), tKeys = new Set<string>(), pairs: Array<[string, string]> = [];
  const seen = new Set<string>();
  for (const f of files) {
    const ast = acorn.parse(Deno.readTextFileSync(f), { ecmaVersion: "latest", sourceType: "module", allowHashBang: true });
    walk(ast, (n) => {
      if (n.type === "Literal" && typeof n.value === "string") literals.add(n.value);
      if (n.type === "TemplateLiteral") for (const q of n.quasis) literals.add(q.value.cooked);
      if (n.type === "BinaryExpression") { const s = fold(n); if (s !== null) literals.add(s); }
      if (n.type === "CallExpression" && n.callee.type === "Identifier" && (n.callee.name === "t" || n.callee.name === "tn")) {
        const a = fold(n.arguments[0]);
        if (a !== null) tKeys.add(a);
        if (n.callee.name === "tn") {
          const b = fold(n.arguments[1]);
          if (a !== null && b !== null && !seen.has(a + "\0" + b)) { seen.add(a + "\0" + b); pairs.push([a, b]); }
          if (b !== null) tKeys.add(b);
        }
      }
    });
  }
  return { literals, pairs, tKeys };
}

// ---- HTML -------------------------------------------------------------------------------------------------------

const ENT: Record<string, string> = { nbsp: " ", amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", hellip: "…", middot: "·", times: "×", rsaquo: "›", laquo: "«", raquo: "»" };
const decode = (s: string) => s.replace(/&(#x?[0-9a-f]+|\w+);/gi, (m, e: string) => {
  if (e[0] === "#") return String.fromCodePoint(e[1].toLowerCase() === "x" ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10));
  return ENT[e] ?? m;
});

/** The English text of web/index.html: attribute values (title, aria-label, placeholder, data-i18n) and text nodes. */
export function scanHtml(): Set<string> {
  const html = Deno.readTextFileSync(`${WEB_ROOT}/index.html`).replace(/<!--[\s\S]*?-->/g, "").replace(/<(script|style)[\s\S]*?<\/\1>/g, "");
  const out = new Set<string>();
  for (const m of html.matchAll(/\b(?:title|aria-label|placeholder|data-i18n|data-i18n-title|data-i18n-html|alt)="([^"]*)"/g)) {
    if (m[1]) out.add(decode(m[1]));
  }
  for (const m of html.matchAll(/>([^<>]+)</g)) {
    const t = decode(m[1]).trim();
    if (t) out.add(t);
  }
  return out;
}

// ---- the web's known English strings -----------------------------------------------------------------------------

export interface WebStrings {
  dicts: Record<string, Record<string, Entry>>;
  pairs: Array<[string, string]>;
  /** norm(string) -> the original spellings, for every English string the web can show */
  known: Map<string, string[]>;
}

export async function loadWeb(): Promise<WebStrings> {
  const dicts = await loadDicts();
  const js = scanJs();
  const known = new Map<string, string[]>();
  const add = (s: string) => {
    const k = argForm(norm(s));
    if (!k) return;
    const l = known.get(k) ?? [];
    if (!l.includes(s)) l.push(s);
    known.set(k, l);
  };
  for (const d of Object.values(dicts)) for (const k of Object.keys(d)) add(k);
  for (const k of Object.keys(loadNative())) add(k);
  for (const s of js.literals) add(s);
  for (const s of scanHtml()) add(s);
  return { dicts, pairs: js.pairs, known };
}

/** The web's `t()` also tries the key with its trailing period toggled; the spellings that reach a string. */
export function webLookup(known: Map<string, string[]>, s: string): string[] | undefined {
  const k = argForm(norm(s));
  return known.get(k) ?? (k.endsWith(".") ? known.get(k.slice(0, -1).trim()) : known.get(k + "."));
}
