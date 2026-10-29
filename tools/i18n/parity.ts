// The parity check between the app and the web's strings (`just i18n-check`, part of `just gate`):
//
//  1. the catalog on disk is what the web's tables generate (gen.ts --check);
//  2. every translation the web has (6 languages) is in the catalog under the key Swift looks up;
//  3. every user-visible string in the app -- the keys the compiler extracted (SWIFT_EMIT_LOC_STRINGS: Text, Button,
//     .help, String(localized:), ...) and the literals handed to `JS.t` / `JS.tn` -- is a string the web shows (a web
//     key), or is listed in native-only.json with the reason the web has none;
//  4. a web string the app shows, which the web translates, is in the catalog under its exact spelling (a different
//     quote or period is an alias for aliases.json);
//  5. every `JS.tn` pair is a pair the web passes to `tn()`;
//  6. every other prose-like string literal in the app is listed in not-ui.json (code, ids, logs, errors the web does
//     not translate either), so a user-visible string cannot slip through unlocalized.
//
//   deno run -A tools/i18n/parity.ts --stringsdata <dir with the app's .stringsdata files>
import { argNames, catalogKey } from "./catalog.ts";
import { argForm, loadNative, loadWeb, LOCALES, norm, webLookup } from "./web.ts";
import { buildCatalog, serialize } from "./catalog.ts";
import { readAliases } from "./gen.ts";

const here = new URL(".", import.meta.url).pathname;
const REPO = `${here}../../`;
const APP = `${REPO}app/SupportFins`;
/** The viewer package cannot call `JS.t`: its literals are keys the app looks up (`JS.t(preset.label)`), so they must be web keys, but are not checked for the call. */
const VIEWER = `${REPO}app/SupportFinsViewer/Sources`;

const args = Deno.args;
const argOf = (name: string) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : undefined; };
const stringsdataDir = argOf("--stringsdata") ?? `${REPO}.dd-ui/Build/Intermediates.noindex/SupportFins.build`;

const errors: string[] = [];
const fail = (m: string) => errors.push(m);

const readJson = (f: string) => JSON.parse(Deno.readTextFileSync(`${here}${f}`));
const nativeRaw = loadNative();
/** argForm(norm(string)) -> entry, so `Plate {n}`, `Plate %lld` and `Plate %arg` are one string */
const nativeOnly = new Map(Object.entries(nativeRaw).map(([k, v]) => [argForm(norm(k)), v]));
const isNative = (k: string) => nativeOnly.has(argForm(norm(k)));
const notUi: Record<string, string> = readJson("not-ui.json");

// ---- the catalog ------------------------------------------------------------------------------------------------
const web = await loadWeb();
const dictKeys = new Set(Object.values(web.dicts).flatMap((d) => Object.keys(d)).map((k) => argForm(norm(k))));
const aliases = readAliases();
const { catalog: want, report } = await buildCatalog(web, aliases);
const catPath = `${APP}/Resources/Localizable.xcstrings`;
const disk = Deno.readTextFileSync(catPath);
if (disk !== serialize(want)) fail(`Localizable.xcstrings is out of date with the web's tables: run \`just i18n-gen\``);
const catalog = JSON.parse(disk) as { strings: Record<string, { localizations?: Record<string, unknown> }> };

// 2. every web translation is in the catalog
let translations = 0;
for (const [lang, locale] of Object.entries(LOCALES)) {
  for (const k of Object.keys(web.dicts[lang])) {
    translations++;
    const ent = catalog.strings[catalogKey(k)] ?? catalog.strings[catalogKey(k, true)];
    if (!ent?.localizations?.[locale]) fail(`web ${lang} translation of "${k}" is missing from the catalog (${locale})`);
  }
}

// ---- the app's strings ------------------------------------------------------------------------------------------
/** a word of three letters or more once the format specifiers are gone */
const hasWord = (s: string) => /[A-Za-z]{3,}/.test(s.replace(/%(?:\d+\$)?(?:lld|ld|d|@|lf|\.\d+f|f|arg)/g, " "));
interface Found { key: string; where: string; kind: "swiftui" | "js.t" | "js.tn" | "other" }
const found: Found[] = [];

// the compiler's list: the strings the code localizes
const compiled = new Set<string>();
/** `file:line:key` of each compiler-extracted literal: the same text elsewhere on another line is not thereby localized */
const compiledAt = new Set<string>();
function walkDir(dir: string, f: (p: string) => void) {
  for (const e of Deno.readDirSync(dir)) {
    const p = `${dir}/${e.name}`;
    if (e.isDirectory) walkDir(p, f); else f(p);
  }
}
let nData = 0;
try {
  walkDir(stringsdataDir, (p) => {
    if (!p.endsWith(".stringsdata")) return;
    const d = JSON.parse(Deno.readTextFileSync(p));
    if (!String(d.source ?? "").includes("/app/SupportFins/")) return;
    nData++;
    for (const [table, items] of Object.entries(d.tables as Record<string, Array<{ key: string; location?: { startingLine: number } }>>)) {
      if (table !== "Localizable") continue;
      for (const it of items) {
        if (!compiled.has(it.key)) found.push({ key: it.key, where: `${String(d.source).split("/app/")[1]}:${it.location?.startingLine ?? "?"}`, kind: "swiftui" });
        compiled.add(it.key);
        compiledAt.add(`${String(d.source).split("/").pop()}:${it.location?.startingLine}:${it.key}`);
      }
    }
  });
} catch (e) { fail(`no .stringsdata under ${stringsdataDir} (build the app first: ${e})`); }
if (nData === 0) fail(`no app .stringsdata under ${stringsdataDir}: build the app first (just app-uibuild)`);

// every string literal of the sources, with where it stands (the compiler's parser, no types)
const proseLike = (s: string) => hasWord(s) && (/\s/.test(s) || /^[A-Z][a-z]+$/.test(s)) && !/^[a-z]+[A-Z]/.test(s);
const swiftFiles: string[] = [];
walkDir(APP, (p) => { if (p.endsWith(".swift")) swiftFiles.push(p); });
walkDir(VIEWER, (p) => { if (p.endsWith(".swift") && !/ViewerBench|Shaders\.swift|FrameBenchmark/.test(p)) swiftFiles.push(p); });
swiftFiles.sort();
const tmp = Deno.makeTempDirSync();
const ex = new Deno.Command("xcrun", { args: ["xcstringstool", "extract", "--all-potential-swift-keys", "-o", tmp, ...swiftFiles], stderr: "piped" }).outputSync();
if (!ex.success) fail("xcstringstool extract failed: " + new TextDecoder().decode(ex.stderr));
const tnPairs: Array<{ s: string; p: string; where: string }> = [];
const other: Found[] = [];
const viewerLits: Found[] = [];
const dec = new TextDecoder();
for (const e of Deno.readDirSync(tmp)) {
  const d = JSON.parse(Deno.readTextFileSync(`${tmp}/${e.name}`));
  const src = d.source as string;
  const bytes = Deno.readFileSync(src);
  const rel = src.split("/app/")[1];
  const lits = ((d.tables.__PotentialKeys ?? []) as Array<{ key: string; location: { startingOffset: number; startingLine: number } }>)
    .sort((a, b) => a.location.startingOffset - b.location.startingOffset);
  let prevTnFirst: string | null = null;
  for (const it of lits) {
    const off = it.location.startingOffset;
    const before = dec.decode(bytes.slice(Math.max(0, off - 240), off));
    const where = `${rel}:${it.location.startingLine}`;
    const m = before.match(/JS\.(tn?)\(\s*$/);
    if (m) {
      found.push({ key: it.key, where, kind: m[1] === "t" ? "js.t" : "js.tn" });
      prevTnFirst = m[1] === "tn" ? it.key : null;
      continue;
    }
    if (prevTnFirst !== null && /",\s*$/.test(before)) {
      tnPairs.push({ s: prevTnFirst, p: it.key, where });
      found.push({ key: it.key, where, kind: "js.tn" });
      prevTnFirst = null;
      continue;
    }
    prevTnFirst = null;
    // a branch of a ternary inside a JS.t( ... ) call: JS.t(own ? "a" : "b", ...)
    if (/[?:]\s*$/.test(before)) {
      const i = Math.max(before.lastIndexOf("JS.t("), before.lastIndexOf("JS.tn("));
      if (i >= 0) {
        const tail = before.slice(i);
        const depth = [...tail].reduce((d, c) => d + (c === "(" ? 1 : c === ")" ? -1 : 0), 0);
        if (depth > 0) { found.push({ key: it.key, where, kind: "js.t" }); continue; }
      }
    }
    // tables of web keys looked up through JS.t / JS.tn by their user: the viewer package and the import note
    if (src.includes("/SupportFinsViewer/") || src.endsWith("/Session/ImportNote.swift")) { viewerLits.push({ key: it.key, where: `${rel}:${it.location.startingLine}`, kind: "other" }); continue; }
    if (!compiledAt.has(`${src.split("/").pop()}:${it.location.startingLine}:${it.key}`)) other.push({ key: it.key, where, kind: "other" });
  }
}
Deno.removeSync(tmp, { recursive: true });

// 3-5. each string the app shows
const seen = new Set<string>();
for (const f of found) {
  const id = f.kind + "\0" + f.key;
  if (seen.has(id)) continue;
  seen.add(id);
  // the key as the catalog spells it
  const isJs = f.kind === "js.t" || f.kind === "js.tn";
  const k = f.key;
  const catKey = isJs ? catalogKey(k, f.kind === "js.tn" && !argNames(k).includes("n")) : k;
  if (!hasWord(k)) continue; // punctuation, numbers and units (mm, ms) need no translation
  const inCatalog = catKey in catalog.strings;
  const inWeb = webLookup(web.known, k) !== undefined;
  if (isNative(k)) continue;
  if (!inWeb && !inCatalog) { fail(`${f.where}: "${k}" is not a web string (no key in the web's tables, JS or index.html). Use the web's wording, or list it in native-only.json with the reason`); continue; }
  if (!inCatalog) {
    // the web has it; if the web translates it, the spelling must match the catalog's exactly
    const alt = (webLookup(web.known, k) ?? []).find((s) => Object.values(web.dicts).some((d) => norm(s) in Object.fromEntries(Object.keys(d).map((x) => [norm(x), 1]))));
    if (alt !== undefined) {
      const dictKey = Object.keys(web.dicts.de).concat(...Object.values(web.dicts).map((d) => Object.keys(d))).find((x) => norm(x) === norm(alt) || argForm(norm(x)) === argForm(norm(k)) || argForm(norm(x)) === argForm(norm(k)) + ".");
      fail(`${f.where}: "${k}" is translated by the web as "${dictKey ?? alt}" but the catalog has no exact key: spell it the same, or add to aliases.json: ${JSON.stringify({ [k]: dictKey ?? alt })}`);
    }
  }
}
for (const t of tnPairs) {
  if (!web.pairs.some(([s, p]) => norm(s) === norm(t.s) && norm(p) === norm(t.p))) fail(`${t.where}: JS.tn("${t.s}", "${t.p}") is not a pair the web passes to tn()`);
}

// the viewer package's literals are keys the app looks up: web strings, or native ones
for (const v of viewerLits) {
  const asWord = hasWord(v.key) && (proseLike(v.key) || dictKeys.has(argForm(norm(v.key))));
  if (!asWord || v.key in notUi || isNative(v.key)) continue;
  if (webLookup(web.known, v.key) === undefined) fail(`${v.where}: "${v.key.slice(0, 90)}" is not a web string (a literal of the viewer package the app shows); use the web's wording, or list it in native-only.json`);
}

// 6. prose-like literals nobody localizes
const unclassified = new Map<string, string>();
for (const o of other) {
  // prose, or a bare word the web shows (`none`, `light grip`): raw values and ids that spell one go in not-ui.json
  if (!(proseLike(o.key) || (hasWord(o.key) && dictKeys.has(argForm(norm(o.key))))) || o.key in notUi || isNative(o.key)) continue;
  unclassified.set(o.key, o.where);
}
for (const [k, w] of unclassified) fail(`${w}: "${k.length > 90 ? k.slice(0, 90) + "…" : k}" is a string literal that is not localized (not a Text/Button/help/String(localized:)/JS.t argument). Localize it, or list it in not-ui.json`);

// stale allowlist entries
const allLiterals = new Set([...compiled, ...found.map((f) => f.key), ...other.map((o) => o.key), ...viewerLits.map((o) => o.key)]);
const literalForms = new Set([...allLiterals].map((k) => argForm(norm(k))));
for (const k of Object.keys(nativeRaw)) if (!literalForms.has(argForm(norm(k)))) fail(`native-only.json: "${k}" is no longer in the app; remove it`);
for (const [k, e] of Object.entries(nativeRaw)) {
  if (e.dev) continue;
  for (const lang of Object.keys(LOCALES)) if (e[lang] === undefined && e.like === undefined) fail(`native-only.json: "${k}" has no ${lang} translation`);
}
for (const k of Object.keys(notUi)) if (!allLiterals.has(k)) fail(`not-ui.json: "${k}" is no longer in the app; remove it`);

// how much of what the app shows each language translates (the web's tables are partial, and the app inherits that)
const shown = [...new Set(found.filter((f) => hasWord(f.key)).map((f) => (f.kind === "swiftui" ? f.key : catalogKey(f.key, f.kind === "js.tn" && !argNames(f.key).includes("n")))))];
const coverage = Object.entries(LOCALES).map(([, loc]) => `${loc} ${shown.filter((k) => catalog.strings[k]?.localizations?.[loc]).length}/${shown.length}`).join(", ");

if (errors.length) {
  console.error(`i18n parity: ${errors.length} problem(s)`);
  for (const e of errors) console.error("  " + e);
  Deno.exit(1);
}
console.log(`i18n parity: app strings translated: ${coverage}\ni18n parity: ok -- catalog ${report.keys} keys, ${translations} web translations present; app strings ${seen.size} checked (${compiled.size} compiler-extracted, ${tnPairs.length} JS.tn pairs), ${nativeOnly.size} native-only, ${Object.keys(notUi).length} not-ui`);
