// Records parity/corpus/readout/updateReadout.jsonl and fmtGrams.jsonl from web/ui/readout.js, the web's
// status readout, so the native strings can be checked against the web's own code.
//
// readout.js is a DOM module. The oracle runs the REAL file from a temp copy of web/, with the modules it
// imports for page state (dom, i18n, settings, part, walls, remove, islands, scene, finbuild, report)
// replaced by small stand-ins that hold what a case sets and record what the readout writes. fins.js,
// fins/fill.js and prop.js are the real ones. i18n is English only (the web's `t`/`tn` for 'en').
//
// updateReadout  args: [built, ctx]  out: {fins: {text, warn}, pad, lead, info: {title, hidden}, time, receipt}
//   built  the build's `built` object as the readout reads it (the fields below are exactly those it reads)
//   ctx    {mode, removed, regions, tineDir, padStyle, padCustomGap, iface, ms, analysisTiming, hasAdded,
//           volumeMm3, wallsPresent, crestPresent, fillReach}: the page state around the build. The JS reads the last
//           four off the fin + pad triangles (`added`), and `fillReach` is the engine's fillReach(bare.coverage): their volume (readout.js meshVolumeMM3), whether
//           there are any, and whether the walls have a tagged crest vertex (`v[3] === 1`).
// fmtGrams       args: [grams]  out: string
import { assertPinned, UPSTREAM_ROOT } from "./lib/pin.js";
import { writeCases } from "./lib/encode.js";
import { upstreamStatus } from "./lib/capture.js";

assertPinned();
const statusBefore = upstreamStatus();

function copyTree(from, to) {
  Deno.mkdirSync(to, { recursive: true });
  for (const e of Deno.readDirSync(from)) {
    if (e.isDirectory) copyTree(`${from}/${e.name}`, `${to}/${e.name}`);
    else if (e.isFile) Deno.copyFileSync(`${from}/${e.name}`, `${to}/${e.name}`);
  }
}

const dir = Deno.makeTempDirSync({ prefix: "sf-readout-" });
copyTree(`${UPSTREAM_ROOT}/web`, `${dir}/web`);
const ui = (name, src) => Deno.writeTextFileSync(`${dir}/web/ui/${name}`, src);

// --- stand-ins for the page ---------------------------------------------------------------------------
ui("dom.js", `
const els = new Map();
export function mk(id) {
  const e = { id, textContent: '', hidden: false, title: '', warn: false, dataset: {},
    querySelector() { return mk('option'); },
    classList: { toggle(c, on) { if (c === 'warn') e.warn = !!on; }, remove(c) { if (c === 'warn') e.warn = false; }, add() {} } };
  return e;
}
export const el = (id) => { if (!els.has(id)) els.set(id, mk(id)); return els.get(id); };
export const reset = () => els.clear();
`);
ui("i18n.js", `
const plural = new Intl.PluralRules('en');
export function t(key, params = {}) {
  if (!key) return '';
  let text = key;
  for (const [k, v] of Object.entries(params)) text = text.replace(new RegExp('{' + k + '}', 'g'), v);
  return text;
}
export function tn(singular, plural1, n, params = {}) {
  return t(plural.select(n) === 'one' ? singular : plural1, { n, ...params });
}
`);
ui("remove.js", `export const removedIds = new Set();`);
ui("walls.js", `
export let drawnWalls = [];
export let drawMsg = '';
export let selectedWall = null;
let note = '';
let shown = false, shape = 'line';
export const selectedNote = () => note;
export const drawShown = () => shown;
export const drawMaterial = {};
export const drawShapeNow = () => shape;
// the page state a case sets: { walls: [{kind, ok, tines, braces, cross, stilt, reason}], msg, selectedNote, shape, shown }
export function __set(d) {
  drawnWalls = (d?.walls ?? []).map((w) => ({ kind: w.kind === 'sway' ? 'sway' : undefined, ok: w.ok,
    info: { tines: w.tines, braces: w.braces, cross: w.cross, stilt: w.stilt, reason: w.reason } }));
  drawMsg = d?.msg ?? '';
  note = d?.selectedNote ?? '';
  selectedWall = d?.selectedNote != null ? {} : null;
  shown = !!d?.shown;
  shape = d?.shape ?? 'line';
}
`);
ui("settings.js", `
export let finMode = 'auto';
export let finsVisible = true;
export const materialDensity = 1.24;
export function syncSectionSums() {}
export const autoLike = (m = finMode) => m === 'auto' || m === 'full';
export function __set(mode) { finMode = mode; }
`);
ui("part.js", `
export const analysisTiming = '5 ms · weld 7 ms';
export const topology = null;   // so the removed-fin bare area is not re-measured (Rust's job)
export let lastResult = { regions: [] };
export const rotM3 = { elements: [1, 0, 0, 0, 1, 0, 0, 0, 1] };
export function __set(regions) { lastResult = { regions: Array.from({ length: regions }, () => ({})) }; }
`);
ui("islands.js", `export function islandsBuilt() {}`);
ui("scene.js", `export const ifaceMaterial = {};`);
ui("finbuild.js", `
export let added = [];
export function __set(a) { added = a; }
export const activeAdded = () => added;
export const activeWalls = () => added;
export const finMaterial = {};
export const padMaterial = {};
`);
ui("report.js", `export function syncReport() {}`);

globalThis.window = { addEventListener() {} }; // readout.js listens for languagechange
const root = `file://${dir}/web/`;
const readout = await import(`${root}ui/readout.js`);
const domMod = await import(`${root}ui/dom.js`);
const settings = await import(`${root}ui/settings.js`);
const wallsMod = await import(`${root}ui/walls.js`);
const part = await import(`${root}ui/part.js`);
const finbuild = await import(`${root}ui/finbuild.js`);
const removeMod = await import(`${root}ui/remove.js`);
const { PROP } = await import(`${root}prop.js`);
const { PAD, FIN } = await import(`${root}fins.js`);
const { fillReach } = await import(`${root}fins/fill.js`);

/** readout.js meshVolumeMM3: |signed volume| of a closed soup, mm^3. */
function volumeOf(tris) {
  let v = 0;
  for (let i = 0; i < tris.length; i += 3) {
    const a = tris[i], b = tris[i + 1], c = tris[i + 2];
    v += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]);
  }
  return Math.abs(v) / 6;
}

function run(built, ctx) {
  domMod.reset();
  settings.__set(ctx.mode);
  part.__set(ctx.regions);
  finbuild.__set(ctx.added ?? []);
  wallsMod.__set(ctx.draw);
  removeMod.removedIds.clear();
  for (let i = 0; i < ctx.removed; i++) removeMod.removedIds.add(i);
  PROP.tineDir = ctx.tineDir;
  PROP.iface = ctx.iface ?? false;
  PAD.style = ctx.padStyle;
  PAD.custom.gap = ctx.padCustomGap ?? 0;
  readout.updateReadout(built, ctx.ms);
  const get = (id) => domMod.el(id);
  return {
    fins: { text: get("s-fins").textContent, warn: get("s-fins").warn },
    pad: get("s-pad").textContent,
    lead: get("s-fin-note").textContent,
    info: { title: get("s-fin-info").title, hidden: get("s-fin-info").hidden },
    time: get("s-time").textContent,
    receipt: { hidden: get("receipt").hidden, grams: get("r-grams").textContent },
  };
}

// --- the cases ---------------------------------------------------------------------------------------------
const tri = (s) => [[0, 0, 0], [s, 0, 0], [0, s, s]]; // a thin wedge: volume 0
function tetra(s) { // a closed tetrahedron of edge s: volume s^3 / 6
  const a = [0, 0, 0], b = [s, 0, 0], c = [0, s, 0], d = [0, 0, s];
  return [[a, c, b], [a, b, d], [a, d, c], [b, c, d]].flat();
}
void tri;

const base = () => ({
  mode: "auto", fins: [{ id: 0 }, { id: 1 }], propCount: 0, braceCount: 2, tines: 8, tallRibs: 0,
  sway: null, seating: { kind: "face" }, pad: null, floating: [], sagRisk: false, fill: null, curvedFill: null,
  curvedDropped: false, bare: null, unserved: 0, smallHoleRegions: 0, skipped: {}, patchStats: {}, patchCount: 2,
  rejected: { blocked: 0, tooFewTines: 0, sites: 2, tried: 2 },
});
const bareAt = (area, total, coverage = 0.5) => ({ area, total, coverage });
const ctx0 = () => ({ mode: "auto", removed: 0, regions: 2, tineDir: "horizontal", padStyle: "auto", padCustomGap: 0, ms: 12.4, added: tetra(10) });

const named = {};
const add = (id, patch = {}, c = {}) => {
  const built = Object.assign(base(), patch);
  named[id] = [built, Object.assign(ctx0(), c, { mode: patch.mode ?? c.mode ?? "auto" })];
};
add("fins-and-tines");
add("one-fin-one-tine", { fins: [{ id: 0 }], braceCount: 1, tines: 1 });
add("fins-no-tines", { tines: 0 });
add("fins-props", { propCount: 3 });
add("props-only", { fins: [{ id: 0 }], braceCount: 0, propCount: 2, tines: 0 });
add("one-prop-only", { fins: [{ id: 0 }], braceCount: 0, propCount: 1, tines: 0 });
add("pins", { tines: 5 }, { tineDir: "vertical" });
add("one-pin", { braceCount: 1, tines: 1 }, { tineDir: "vertical" });
add("ribs", { tallRibs: 3 });
add("one-rib", { tallRibs: 1 });
add("removed", {}, { removed: 2 });
add("none-possible", { fins: [], braceCount: 0, propCount: 0, tines: 0, patchCount: 0, rejected: { blocked: 0, tooFewTines: 0, sites: 0, tried: 0 }, patchStats: { tooNarrow: 3 } });
add("none-possible-flat", { fins: [], braceCount: 0, propCount: 0, tines: 0, patchCount: 0, patchStats: { notFlat: 4, tooNarrow: 1 } });
add("none-too-high", { fins: [], braceCount: 0, propCount: 0, tines: 0, patchCount: 2, patchStats: { tooHigh: 2 }, rejected: { blocked: 0, tooFewTines: 0, sites: 0, tried: 0 } });
add("none-too-high-one", { fins: [], braceCount: 0, propCount: 0, tines: 0, patchCount: 1, patchStats: { tooHigh: 1 }, rejected: { blocked: 0, tooFewTines: 0, sites: 0, tried: 0 } });
add("none-no-usable", { fins: [], braceCount: 0, propCount: 0, tines: 0, patchCount: 1, patchStats: {}, rejected: { blocked: 0, tooFewTines: 0, sites: 0, tried: 0 } });
add("none-blocked", { fins: [], braceCount: 0, propCount: 0, tines: 0, rejected: { blocked: 2, tooFewTines: 0, sites: 2, tried: 2 } });
add("none-inside", { fins: [], braceCount: 0, propCount: 0, tines: 0, rejected: { blocked: 0, tooFewTines: 1, sites: 1, tried: 1 } });
add("none-point", { fins: [], braceCount: 0, propCount: 0, tines: 0, seating: { kind: "point", span: 0.5, bedArea: 0 }, pad: null });
add("none-plate-only", { fins: [], braceCount: 0, propCount: 0, tines: 0, skipped: { onPart: 3 } });
add("some-plate-only", { skipped: { onPart: 1 } });
add("small-holes-only", { fins: [], braceCount: 0, propCount: 0, tines: 0, smallHoleRegions: 2 }, { regions: 2 });
add("small-hole-only-one", { fins: [], braceCount: 0, propCount: 0, tines: 0, smallHoleRegions: 1 }, { regions: 1 });
add("small-holes-and-more", { fins: [], braceCount: 0, propCount: 0, tines: 0, smallHoleRegions: 1 }, { regions: 3 });
add("small-holes-with-fins", { smallHoleRegions: 2 });
add("point-with-pad", { seating: { kind: "point", span: 0.1, bedArea: 0 }, pad: { style: "sure", autoSure: false, smallFoot: false, outline: 3 } });
add("point-without-pad", { seating: { kind: "point", span: 0.1, bedArea: 0 }, pad: null });
add("pad-added", { pad: { style: "light", autoSure: false, smallFoot: false, outline: 80 } });
add("pad-sure-auto", { pad: { style: "sure", autoSure: true, smallFoot: true, outline: 7.4 } });
add("pad-small-foot-light", { pad: { style: "light", autoSure: false, smallFoot: true, outline: 0.5 } }, { padStyle: "light" });
add("pad-small-foot-light-mm", { pad: { style: "light", autoSure: false, smallFoot: true, outline: 12.6 } }, { padStyle: "light" });
add("pad-small-foot-custom-gap", { pad: { style: "light", autoSure: false, smallFoot: true, outline: 3 } }, { padStyle: "custom", padCustomGap: 0.1 });
add("pad-small-foot-custom-nogap", { pad: { style: "light", autoSure: false, smallFoot: true, outline: 3 } }, { padStyle: "custom", padCustomGap: 0 });
add("pad-small-foot-auto-light", { pad: { style: "light", autoSure: false, smallFoot: true, outline: 3 } }, { padStyle: "auto" });
add("floating-one", { floating: [{ faces: 4, lowest: [0, 0, 5], drop: 5.55 }] });
add("floating-two", { floating: [{ faces: 4, lowest: [0, 0, 5], drop: 2.04 }, { faces: 1, lowest: [0, 0, 9], drop: 9 }] });
add("sag-risk", { sagRisk: true });
add("unserved-one", { unserved: 1 });
add("unserved-many", { unserved: 5 });
add("unserved-with-fill", { unserved: 5, fill: { walls: 1, bareBefore: 20, unservedArea: 0.2, capped: false } }, { mode: "full" });
add("small-hole-ceilings", { smallHoleRegions: 3 });
add("sway-none", { sway: { count: 0, tines: 0, skipped: 0, cross: 0, reason: "no tall upright side" } });
add("sway-some", { sway: { count: 2, tines: 14, skipped: 0, cross: 0 } });
add("sway-all", { sway: { count: 1, tines: 7, skipped: 2, cross: 3 } });
add("sway-no-tines", { sway: { count: 3, tines: 0, skipped: 1, cross: 0 } });
add("sway-and-nothing-else", { fins: [], braceCount: 0, propCount: 0, tines: 0, sway: { count: 2, tines: 4, skipped: 0, cross: 0 } });
add("iface-flat-none", {}, { iface: "flat", added: tetra(10) });
// Full coverage
const fullBase = { mode: "full" };
add("full-added", { ...fullBase, fill: { walls: 3, bareBefore: 100, unservedArea: 0, capped: false, islandsBare: 0, islandsCapped: false } });
add("full-added-one", { ...fullBase, fill: { walls: 1, bareBefore: 30, unservedArea: 0, capped: false } });
add("full-nothing-added", { ...fullBase, fill: { walls: 0, bareBefore: 0.4, unservedArea: 0, capped: false } });
add("full-nothing-added-bare", { ...fullBase, fill: { walls: 0, bareBefore: 30, unservedArea: 30, capped: false } });
add("full-bare-left", { ...fullBase, fill: { walls: 2, bareBefore: 90, unservedArea: 12.4, capped: false } });
add("full-capped", { ...fullBase, fill: { walls: 5, bareBefore: 400, unservedArea: 250.5, capped: true } });
add("full-bare-subone", { ...fullBase, fill: { walls: 2, bareBefore: 90, unservedArea: 0.4, capped: false } });
add("full-mostly-bare", { ...fullBase, fill: { walls: 2, bareBefore: 400, unservedArea: 120, capped: false }, bare: bareAt(120, 200) });
add("full-bare-share", { ...fullBase, fill: { walls: 2, bareBefore: 90, unservedArea: 40, capped: false }, bare: bareAt(40, 200) });
add("full-curved-dropped", { ...fullBase, curvedDropped: true, fill: { walls: 1, bareBefore: 50, unservedArea: 0, capped: false } });
// Auto's bare share and the figure warning
add("auto-bare-small", { bare: bareAt(60, 400) });
add("auto-bare-tiny", { bare: bareAt(0.4, 400) });
add("auto-mostly-bare", { bare: bareAt(150, 200) });
add("auto-mostly-bare-small-area", { bare: bareAt(20, 25) });
add("auto-mostly-bare-sparse", { bare: bareAt(150, 200, 0.2) });
add("auto-mostly-bare-no-fins", { fins: [], braceCount: 0, propCount: 0, tines: 0, bare: bareAt(150, 200) });
add("auto-bare-min-pct", { bare: bareAt(2, 100000) });
add("auto-bare-reach-low", { bare: bareAt(60, 400, 0) });
add("auto-bare-reach-high", { bare: bareAt(60, 400, 1) });
// Curved walls in Auto
add("curved-fill", { curvedFill: { walls: 2, unservedArea: 0, capped: false } });
add("curved-fill-one", { curvedFill: { walls: 1, unservedArea: 3.2, capped: false } });
add("curved-capped", { curvedFill: { walls: 4, unservedArea: 41.6, capped: true } });
add("curved-no-walls-bare", { curvedFill: { walls: 0, unservedArea: 7, capped: false } });
add("full-and-curved-fill", { ...fullBase, fill: { walls: 1, bareBefore: 40, unservedArea: 6, capped: false }, curvedFill: { walls: 1, unservedArea: 9, capped: false } });
// Draw mode (updateDrawReadout): the hand-drawn walls, and what the pad and seating say beside them
const drawBase = { mode: "draw", fins: [], braceCount: 0, propCount: 0, tines: 0, patchCount: 0, rejected: { blocked: 0, tooFewTines: 0, sites: 0, tried: 0 } };
const w = (o = {}) => ({ kind: "wall", ok: true, tines: 0, braces: 0, cross: 0, stilt: 0, ...o });
const br = (o = {}) => ({ kind: "sway", ok: true, tines: 0, braces: 0, cross: 0, stilt: 0, ...o });
const dr = (walls, o = {}) => ({ draw: { walls, msg: "", shape: "line", shown: true, ...o } });
add("draw-empty-line", drawBase, dr([]));
add("draw-empty-curve", drawBase, dr([], { shape: "curve" }));
add("draw-one-wall", drawBase, dr([w({ tines: 9 })]));
add("draw-one-wall-no-tines", drawBase, dr([w()]));
add("draw-one-tine", drawBase, dr([w({ tines: 1 })]));
add("draw-pins", drawBase, { ...dr([w({ tines: 6 }), w({ tines: 4 })]), tineDir: "vertical" });
add("draw-walls-ribs", drawBase, dr([w({ tines: 5, braces: 2 }), w({ tines: 3, braces: 1 })]));
add("draw-one-wall-one-rib", drawBase, dr([w({ tines: 5, braces: 1 })]));
add("draw-braces-cross", drawBase, dr([br({ tines: 12, cross: 2 }), br({ tines: 8, cross: 1 })]));
add("draw-one-brace", drawBase, dr([br({ tines: 7 })]));
add("draw-brace-stilt", drawBase, dr([br({ tines: 7, stilt: 31.6 })]));
add("draw-braces-stilt", drawBase, dr([br({ tines: 7, stilt: 25 }), br({ tines: 9, stilt: 41.4 }), br({ tines: 3, stilt: 5 })]));
add("draw-mixed", drawBase, dr([w({ tines: 4, braces: 1 }), br({ tines: 9, cross: 1 }), w({ tines: 4 })]));
add("draw-bad-one", drawBase, dr([w({ tines: 4 }), w({ ok: false, reason: "wall too short — 0.5mm, needs 2mm" })]));
add("draw-bad-no-reason", drawBase, dr([w({ ok: false })]));
add("draw-bad-many", drawBase, dr([w({ ok: false, reason: "nothing to hold up there — the line sits at the plate" }), br({ ok: false, reason: "that’s too low for a brace — move up the side" })]));
add("draw-msg-only", drawBase, dr([], { msg: "couldn’t place that wall: wall too short — 1.0mm, needs 2mm" }));
add("draw-msg-with-wall", drawBase, dr([w({ tines: 4 })], { msg: "can’t place this curve: curve too tight — 3.0mm radius, needs 5mm" }));
add("draw-selected", drawBase, dr([w({ tines: 4 })], { selectedNote: "selected: wall 24mm long, 4 tines. Press Delete or Remove selected to take it out (Esc to keep it)" }));
add("draw-point-with-pad", { ...drawBase, seating: { kind: "point", span: 0.1, bedArea: 0 }, pad: { style: "sure", autoSure: false, smallFoot: false, outline: 3 } }, dr([w({ tines: 4 })]));
add("draw-point-without-pad", { ...drawBase, seating: { kind: "point", span: 0.1, bedArea: 0 }, pad: null }, dr([]));
add("draw-pad-small-foot", { ...drawBase, pad: { style: "light", autoSure: false, smallFoot: true, outline: 3 } }, { ...dr([w({ tines: 4 })]), padStyle: "light" });
add("draw-pad-sure-auto", { ...drawBase, pad: { style: "sure", autoSure: true, smallFoot: true, outline: 7.4 } }, dr([w()]));
add("draw-time", drawBase, { ...dr([w({ tines: 4 })]), ms: 41.6 });
add("draw-no-ms", drawBase, { ...dr([w({ tines: 4 })]), ms: null });
add("draw-iface-flat-none", drawBase, { ...dr([w({ tines: 4 })]), iface: "flat" });
add("draw-no-added", drawBase, { ...dr([]), added: [] });
add("draw-grams", drawBase, { ...dr([w({ tines: 4 })]), added: tetra(35) });
// "+ Add walls by hand" in Auto and Full (updateFinReadout): the hand-added walls beside the auto ones
add("aug-fins-and-drawn", {}, dr([w({ tines: 4 }), w({ tines: 2 })]));
add("aug-one-drawn", {}, dr([w({ tines: 4 })]));
add("aug-only-drawn", { fins: [], braceCount: 0, propCount: 0, tines: 0, patchCount: 0, rejected: { blocked: 0, tooFewTines: 0, sites: 0, tried: 0 } }, dr([w({ tines: 4 })]));
add("aug-drawn-and-sway", { sway: { count: 2, tines: 14, skipped: 0, cross: 1 } }, dr([w({ tines: 4 })]));
add("aug-bad", {}, dr([w({ tines: 4 }), w({ ok: false, reason: "wall too short — 0.5mm, needs 2mm" })]));
add("aug-bad-many", {}, dr([w({ ok: false }), br({ ok: false, reason: "that face is too small or curved to stand a brace against" })]));
add("aug-msg-and-selected", {}, dr([w({ tines: 4 })], { msg: "couldn’t place that brace: that’s too low for a brace — move up the side", selectedNote: "selected: sway brace 40mm tall. Press Delete or Remove selected to take it out (Esc to keep it)" }));
add("aug-hidden", {}, dr([w({ tines: 4 })], { shown: false }));
add("aug-none-possible-only-bad", { fins: [], braceCount: 0, propCount: 0, tines: 0, patchCount: 0, rejected: { blocked: 0, tooFewTines: 0, sites: 0, tried: 0 } }, dr([w({ ok: false, reason: "x" })]));
add("aug-small-holes-with-drawn", { fins: [], braceCount: 0, propCount: 0, tines: 0, smallHoleRegions: 2 }, { ...dr([w({ tines: 4 })]), regions: 2 });
add("aug-small-holes-with-bad-drawn", { fins: [], braceCount: 0, propCount: 0, tines: 0, smallHoleRegions: 2 }, { ...dr([w({ ok: false })]), regions: 2 });
add("aug-full", { mode: "full", fill: { walls: 2, bareBefore: 90, unservedArea: 12.4, capped: false } }, dr([w({ tines: 4 })]));
add("aug-point-seating", { seating: { kind: "point", span: 0.1, bedArea: 0 }, pad: { style: "sure", autoSure: false, smallFoot: false, outline: 3 } }, dr([w({ tines: 4 })]));
// many things at once
add("kitchen-sink", {
  propCount: 2, tallRibs: 2, floating: [{ faces: 1, lowest: [0, 0, 3], drop: 3 }], sagRisk: true, unserved: 0,
  pad: { style: "light", autoSure: false, smallFoot: true, outline: 2 }, smallHoleRegions: 1, skipped: { onPart: 2 },
  sway: { count: 0, tines: 0, skipped: 0, cross: 0, reason: "the part is in the way" }, bare: bareAt(150, 200),
  curvedFill: { walls: 1, unservedArea: 5, capped: false },
}, { padStyle: "light", removed: 1 });
add("no-ms", {}, { ms: null });
add("no-added", {}, { added: [] });
add("big-grams", {}, { added: tetra(60) });
add("mid-grams", {}, { added: tetra(35) });

const cases = Object.entries(named).map(([id, [built, ctx]]) => {
  const out = run(built, ctx);
  const { added, ...rest } = ctx; // the triangles themselves are not an input of the native readout
  const pub = { ...rest, ...(built.bare ? { fillReach: fillReach(built.bare.coverage ?? FIN.coverDefault) } : {}), analysisTiming: "5 ms · weld 7 ms", hasAdded: added.length > 0, volumeMm3: volumeOf(added),
    wallsPresent: added.length > 0, crestPresent: added.some((v) => v[3] === 1) };
  return { id, args: [built, pub], out };
});
writeCases("readout", "updateReadout", cases);

// fmtGrams: one decimal under 9.95 g, else rounded
const grams = [0, 0.04, 0.05, 0.94, 1, 2.449, 5.5, 9.9, 9.94, 9.949999, 9.95, 9.951, 10, 10.4, 10.5, 99.5, 123.456, 1000.4, 0.0001];
writeCases("readout", "fmtGrams", grams.map((g) => ({ id: String(g), args: [g], out: readout.fmtGrams(g) })));

// toFixed1: V8's (1).toFixed on exact ties (odd multiples of 0.25 go to the larger n) and near-ties
const fixed = [0, 0.04, 0.05, 0.25, 0.75, 1.25, 1.005, 2.25, 2.35, 5.55, 9.95, 9.949999, 12.25, 100.75, 1234.5];
writeCases("readout", "toFixed1", fixed.map((x) => ({ id: String(x), args: [x], out: x.toFixed(1) })));

console.log("readout:", cases.length, "updateReadout cases;", grams.length, "fmtGrams cases");
Deno.removeSync(dir, { recursive: true });
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
