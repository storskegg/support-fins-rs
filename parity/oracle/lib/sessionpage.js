// The page's session code (web/ui/session.js) run in Deno on the page's own text, for parity/oracle/session.js (which
// records the corpus) and parity/cli/session_check.js (which reads files the native app wrote). See session.js.
import { UPSTREAM_ROOT, WEB, assertPinned } from "./pin.js";

assertPinned();
export const WEBDIR = `${UPSTREAM_ROOT}/web/`;
export const THREE = await import(`${WEB}vendor/three/three.core.js`);
const { buildTopology } = await import(`${WEB}overhangs.js`);
const { readThreeMF } = await import(`${WEB}threemf.js`);

// ---------------------------------------------------------------------------------------------- the page's fields
const html = Deno.readTextFileSync(`${WEBDIR}index.html`);
const attr = (tag, name) => {
  const m = new RegExp(`(?:^|\\s)${name}(?:="([^"]*)")?(?=\\s|/?>|$)`).exec(tag);
  return m ? (m[1] ?? "") : undefined;
};
export const elements = new Map();
for (const m of html.matchAll(/<(input|select|button|textarea|div|span|section|p|label)\b([^>]*?)(\/?)>/g)) {
  const [, tagName, rest] = m;
  const id = attr(rest, "id");
  if (id === undefined || elements.has(id)) continue;
  const e = { id, tagName: tagName.toUpperCase(), type: tagName === "input" ? (attr(rest, "type") ?? "text") : tagName === "select" ? "select-one" : "",
    min: attr(rest, "min"), max: attr(rest, "max"), step: attr(rest, "step"), value: attr(rest, "value"), checked: attr(rest, "checked") !== undefined,
    options: [] };
  if (tagName === "select") {
    const end = html.indexOf("</select>", m.index);
    const body = html.slice(m.index + m[0].length, end);
    for (const o of body.matchAll(/<option\b([^>]*)>/g)) e.options.push({ value: attr(o[1], "value"), selected: attr(o[1], "selected") !== undefined });
    e.value = (e.options.find((o) => o.selected) ?? e.options[0])?.value;
  }
  elements.set(id, e);
}
globalThis.document = { getElementById: (id) => elements.get(id) ?? null };

const history = Deno.readTextFileSync(`${WEBDIR}ui/history.js`);
const listOf = (re) => JSON.parse(re.exec(history)[1].replace(/'/g, '"'));
export const FORM_IDS = listOf(/const FORM_IDS = (\[[\s\S]*?\]);/);
export const VIEWER_IDS = listOf(/export const VIEWER_IDS = new Set\((\[[\s\S]*?\])\)/);

// ---------------------------------------------------------------------------------------------- the real session.js
const dir = Deno.makeTempDirSync({ prefix: "sf-session-" });
const stub = (name, src) => Deno.writeTextFileSync(`${dir}/${name}.mjs`, src);
stub("dom", Deno.readTextFileSync(`${WEBDIR}ui/dom.js`));
stub("part", `export let part = null, topology = null, lastResult = null; export const rotM3 = { elements: null };
export function setPage(p, t) { part = p; topology = t; }`);
stub("walls", `export const drawnWalls = [];`);
stub("settings", `export const finMode = 'auto', finsVisible = true;`);
stub("remove", `export const removedSigs = new Set();`);
stub("history", `export const VIEWER_IDS = new Set(${JSON.stringify(VIEWER_IDS)});
export const sessionForm = () => ({});
export const resetHistory = () => { globalThis.__reset = true; };
export function restoreState(s) { globalThis.__restored = s; }`);
let src = Deno.readTextFileSync(`${WEBDIR}ui/session.js`);
const rewrites = [["'three'", `'${WEB}vendor/three/three.core.js'`], ["'./dom.js'", "'./dom.mjs'"], ["'./part.js'", "'./part.mjs'"],
  ["'./walls.js'", "'./walls.mjs'"], ["'./settings.js'", "'./settings.mjs'"], ["'./remove.js'", "'./remove.mjs'"], ["'./history.js'", "'./history.mjs'"]];
for (const [from, to] of rewrites) {
  if (!src.includes(`from ${from}`)) throw new Error(`session.js no longer imports ${from}`);
  src = src.replace(`from ${from}`, `from ${to}`);
}
stub("session", src);
export const session = await import(`${dir}/session.mjs`);
const pageState = await import(`${dir}/part.mjs`);

// ---------------------------------------------------------------------------------------------- the import note
const io = Deno.readTextFileSync(`${WEBDIR}ui/io.js`);
const a = io.indexOf("export function renderImportNote()");
const b = io.indexOf("}   // what the 3MF/STEP reader had to decide");
if (a < 0 || b < 0) throw new Error("renderImportNote slice not found");
stub("note", `
const t = (key, params = {}) => Object.entries(params).reduce((s, [k, v]) => s.replace(new RegExp('{' + k + '}', 'g'), v), key);
const tn = (one, other, n) => t(n === 1 ? one : other, { n });
export let lastImportMeta = null, importNote = '';
export function setMeta(m) { lastImportMeta = m; }
${io.slice(a, b + 1).replace("export function renderImportNote", "export function renderImportNote")}
`);
const note = await import(`${dir}/note.mjs`);

// ---------------------------------------------------------------------------------------------- the page's parts
/** setPart's centring (ui/part.js:81-90): the loaded Float32Array, centred on its box in all three axes. */
export function setPart(positions) {
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(positions.slice(), 3));
  geometry.computeBoundingBox();
  const bb = geometry.boundingBox;
  const centre = [(bb.min.x + bb.max.x) / 2, (bb.min.y + bb.max.y) / 2, (bb.min.z + bb.max.z) / 2];
  geometry.translate(-centre[0], -centre[1], -centre[2]);
  geometry.computeBoundingBox();
  const topo = buildTopology({ getAttribute: (k) => (k === "position" ? geometry.getAttribute("position") : null) });
  return { geometry, topo, centre };
}
const exportSrc = Deno.readTextFileSync(`${WEBDIR}ui/export.js`);
const toPrintSrc = exportSrc.slice(exportSrc.indexOf("  const toPrint = (flat"), exportSrc.indexOf("  const partTris = toPrint(pos);"));
if (!toPrintSrc.includes("toPrint")) throw new Error("toPrint slice not found");
export const toPrintOf = new Function("rot", "dx", "dy", "dz", `${toPrintSrc}\n return toPrint;`);


/** The fields' values on a fresh page (readForm on load), in FORM_IDS order, less the viewer's. */
export function defaultForm() {
  const form = {};
  for (const id of FORM_IDS) {
    if (VIEWER_IDS.includes(id)) continue;
    const f = elements.get(id);
    form[id] = f.type === "checkbox" ? f.checked : f.value;
  }
  return form;
}

const arr3 = (v) => [v.x, v.y, v.z];

/** Read `bytes` as the page does (ui/io.js parseModel + afterLoad) and say what the session restored. */
export async function readBack(bytes, beside) {
  const { objects, unit, skipped, session: saved } = await readThreeMF(new Uint8Array(bytes));
  if (!saved) return { session: "none", objects: objects.length };
  const own = session.partObjectOf(objects);
  if (!own) return { session: "none", objects: objects.length, noOwn: true };
  const { geometry, topo, centre } = setPart(own.positions);
  pageState.setPage({ geometry }, topo);
  globalThis.__restored = null;
  globalThis.__reset = false;
  const r = session.restoreSession(saved, beside);
  const meta = { type: "3MF", objectsLength: objects.length, chosenLength: 1, chosenName: own.name, meshes: own.meshes ?? 1, skipped,
    extras: 0, cut: 0, uncut: 0, why: null, unit, session: r.why ?? (beside ? "beside" : "restored"), restored: r.n ?? null };
  note.setMeta(meta);
  const text = note.renderImportNote();
  note.setMeta({ ...meta, session: null, restored: null });
  const scaled = note.renderImportNote();
  const out = { session: r.why ?? "restored", objects: objects.length, n: r.n ?? null, centre, note: text, noteScaled: scaled };
  if (!r.why) {
    const s = globalThis.__restored;
    if (!globalThis.__reset) throw new Error("a restore did not reset the history");
    out.state = {
      walls: s.walls.map((w) => ({ ...(w.kind ? { kind: w.kind } : {}), ...(w.kind === "sway" ? { face: w.face } : {}), capped: w.capped === true,
        a: arr3(w.a), ...(w.b ? { b: arr3(w.b) } : {}), ...(w.m ? { m: arr3(w.m) } : {}) })),
      removedSigs: s.removedSigs, finMode: s.finMode, finsVisible: s.finsVisible,
      form: Object.fromEntries(Object.entries(s.form).filter(([id]) => FORM_IDS.includes(id))),
    };
    for (const k of ["quat", "load", "drawAugment"]) out.state[k] = k === "quat" ? s.quat : s[k];
  }
  return out;
}

