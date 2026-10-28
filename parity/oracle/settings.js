// Records parity/corpus/settings/optionsJSON.jsonl: what the native app's FinSettings.optionsJSON() must
// produce, from the web's own behaviour.
//   optionsJSON  args: [form]  out: {dialog, options}
// `form` is a FinSettings as the app's Codable writes it, but only the fields that differ from the web
// defaults (the Swift decoder fills in the rest). `dialog` is the `{options.json key: value}` object the app
// must hand the engine (`opts_json`): the web's finOpts / swayOpts (web/ui/finbuild.js:115-160) restated in
// the entry's units (a percent slider's value / 100) and keyed by options.json (plugins/shared/engine/
// options.json). `options` is what the engine entry's optionsFromDialog (plugins/shared/engine/
// fins_entry.js:60-103) makes of that dialog, so the case also proves the dialog is one the engine accepts
// (it refuses a value out of range rather than guess).
// `dialog` also carries the four request extras that are neither options.json keys nor tunables (docs/FFI.md):
// tallRibs, curvedWalls, sway.uncappedDepth, sway.crossRise; they are taken out before optionsFromDialog.
// `finOpts` is the page's own finOpts() (see "the page" below) for the form; its `tunables` are: rounded feet, the pad's
// numbers, the gap, tall-rib height, cutout, interface material, tine direction, pin depth, nozzle, wall lines.
// Display toggles are not part of a build.
import { assertPinned, UPSTREAM_ROOT } from "./lib/pin.js";
import { writeCases } from "./lib/encode.js";
import { upstreamStatus } from "./lib/capture.js";

assertPinned();
const statusBefore = upstreamStatus();
const { optionsFromDialog } = await import(`file://${UPSTREAM_ROOT}/plugins/shared/engine/fins_entry.js`);

// the web's form defaults (docs/UI-MAP.md 4.1-4.2; index.html)
const DEFAULTS = {
  mode: "auto", threshold: 45, nozzle: 0.4, layerHeight: 0.2, gap: 0.2, interfaceMaterial: "off",
  tines: true, tineDensity: 0, tineDirection: "horizontal", pinDepth: 0.1, cutout: "none",
  roundedFeet: false, bedPad: "auto", padThickness: 0.2, padGap: 0.12, padGrip: 0, padSpread: 4, wallLines: 2,
  tallRibs: true, tallRibsHeight: 10, sway: false, swayFrom: 0, swaySpacing: 6, swayDepth: 15,
  fullBraceDepth: false, swayCrossHeight: 50,
  coverage: 50, plateOnly: false, separateWalls: false, longWalls: true, curvedWalls: false,
  highlightSmall: true, showIslands: false,
};

/** The dialog the web's finOpts / swayOpts send for a form (see the header). */
function dialogOf(form) {
  const f = { ...DEFAULTS, ...form };
  const d = {};
  if (f.mode !== "draw") d.mode = f.mode;                       // Draw is drawn by hand: no auto request
  d.threshold = f.threshold;
  d.nozzle = f.nozzle;
  d.tines = f.tines;
  d.tineDensity = f.tineDensity / 100;
  d.tineDir = f.tineDirection;
  d.pinDepth = f.pinDepth;
  d.layerHeight = f.layerHeight;
  d.gap = f.gap;
  if (f.bedPad !== "custom") d.padStyle = f.bedPad;             // Custom: four hand-typed numbers (tunables)
  d.cutout = f.cutout;
  d.coverage = f.mode === "full" ? 0.5 : f.coverage / 100;      // Full coverage aims at every reachable overhang
  d.plateOnly = f.plateOnly;
  d.separateWalls = f.separateWalls ? "on" : "off";
  d.longWalls = f.longWalls;
  d.tallRibs = f.tallRibs;                                      // finOpts: the "Brace tall walls" box
  d.curvedWalls = f.curvedWalls && f.mode !== "draw";           // autoLike() && checked
  const sway = f.sway && f.mode !== "draw";                     // Auto places sway braces itself; Draw clicks them
  d["sway.on"] = sway;
  if (sway) {
    d["sway.gripFrom"] = f.swayFrom;
    d["sway.tineSpacing"] = f.swaySpacing;
    d["sway.reach"] = f.swayDepth / 100;
    d["sway.uncappedDepth"] = f.fullBraceDepth;
    d["sway.crossRise"] = Math.max(10, Math.min(90, f.swayCrossHeight)) / 100;   // swayOpts
  }
  return d;
}


// --- the page: the REAL ui/settings.js and ui/finbuild.js on a stub DOM ---------------------------------
// finOpts() (web/ui/finbuild.js:115) reads the page's controls and the module state ui/settings.js keeps from
// them (FIN, PAD, PROP, BRACE, CUT, nozzle.js). We load both unmodified from a temp copy of web/, with the
// rest of the page (scene, part, walls, history, ...) replaced by no-op stand-ins and `three` by a stub, give
// the controls the defaults parsed from web/index.html, set them for a form, run initSettings() and the
// controls' own handlers, and record finOpts() whole.
function copyTree(from, to) {
  Deno.mkdirSync(to, { recursive: true });
  for (const e of Deno.readDirSync(from)) {
    if (e.isDirectory) copyTree(`${from}/${e.name}`, `${to}/${e.name}`);
    else if (e.isFile) Deno.copyFileSync(`${from}/${e.name}`, `${to}/${e.name}`);
  }
}
const dir = Deno.makeTempDirSync({ prefix: "sf-settings-" });
copyTree(`${UPSTREAM_ROOT}/web`, `${dir}/web`);
const ui = (name, src) => Deno.writeTextFileSync(`${dir}/web/ui/${name}`, src);

// the controls and their defaults, from index.html
const html = Deno.readTextFileSync(`${UPSTREAM_ROOT}/web/index.html`);
const attr = (tag, name) => tag.match(new RegExp(`(?:^|\\s)${name}(?:="([^"]*)")?(?=[\\s>/])`))?.[1] ?? (new RegExp(`\\s${name}(?=[\\s>/])`).test(tag) ? "" : undefined);
const controls = {};
for (const m of html.matchAll(/<input\b[^>]*>/g)) {
  const id = attr(m[0], "id");
  if (id) controls[id] = { kind: "input", type: attr(m[0], "type"), value: attr(m[0], "value") ?? "", checked: attr(m[0], "checked") !== undefined, min: attr(m[0], "min"), max: attr(m[0], "max") };
}
for (const m of html.matchAll(/<select\b[^>]*id="([^"]+)"[^>]*>([\s\S]*?)<\/select>/g)) {
  const options = [...m[2].matchAll(/<option\b([^>]*)>/g)].map((o) => ({ value: attr(o[1], "value"), selected: attr(o[1], "selected") !== undefined }));
  controls[m[1]] = { kind: "select", options, value: (options.find((o) => o.selected) ?? options[0])?.value ?? "" };
}
globalThis.__controls = controls;

ui("dom.js", `
const els = new Map(), listeners = new Map();
const spec = globalThis.__controls;
function make(id) {
    const c = spec[id] ?? { value: '' };
    const e = { id, value: c.value, checked: !!c.checked, hidden: false, title: '', textContent: '', dataset: {},
      min: c.min, max: c.max, type: c.type,
      options: c.options?.map((o) => ({ value: o.value, disabled: false })),
      classList: { add() {}, remove() {}, toggle() {}, contains() { return false; } },
      querySelector() { return { hidden: false, textContent: '', classList: { add() {}, remove() {}, toggle() {} } }; },
      setAttribute() {}, removeAttribute() {}, focus() {},
      addEventListener(type, fn) { const k = id + ':' + type; (listeners.get(k) ?? listeners.set(k, []).get(k)).push(fn); } };
    Object.defineProperty(e, 'valueAsNumber', { get() { return e.value === '' ? NaN : Number(e.value); } });
    return new Proxy(e, { get: (t, k) => (k in t || typeof k === 'symbol' ? t[k] : () => make('anon')) });
}
export function reset() { els.clear(); }
export const el = (id) => { if (!els.has(id)) els.set(id, make(id)); return els.get(id); };
export const fire = (id, type) => { for (const fn of listeners.get(id + ':' + type) ?? []) { try { fn({ isTrusted: false, target: els.get(id) }); } catch (e) { if (!/stand-in/.test(e?.message)) throw e; } } };
`);
// every other page module the two import: its exports become no-ops
const noop = "export const __noop = new Proxy(function () {}, { get: (_, k) => (k === Symbol.toPrimitive ? () => '' : __noop), apply: () => __noop, construct: () => __noop });\n";
const stub = (name, overrides = "") => {
  const src = Deno.readTextFileSync(`${UPSTREAM_ROOT}/web/ui/${name}`);
  const names = new Set([...src.matchAll(/export\s+(?:async\s+)?(?:function\*?|const|let|class)\s+([A-Za-z_$][\w$]*)/g)].map((m) => m[1]));
  for (const m of src.matchAll(/export\s*\{([^}]*)\}/g)) for (const n of m[1].split(",")) names.add(n.trim().split(/\s+as\s+/).pop());
  ui(name, noop + [...names].filter((n) => n && !new RegExp(`\\b${n.replace("$", "\\$")}\\b`).test(overrides)).map((n) => `export const ${n} = __noop;`).join("\n") + "\n" + overrides);
};
stub("history.js"); stub("remove.js", "export const removeMode = false;\n");
stub("walls.js"); stub("pose.js"); stub("part.js", "export const topology = null, lastResult = null, rotM3 = { elements: [1,0,0,0,1,0,0,0,1] };\n");
stub("more.js"); stub("scene.js"); stub("readout.js"); stub("parkedbuild.js"); stub("report.js");
stub("islands.js", "export const islands = [];\n");
ui("i18n.js", "export const t = (k) => k; export const tn = (a, b, n) => (n === 1 ? a : b);\n");
Deno.writeTextFileSync(`${dir}/web/ui/threestub.js`, noop + "export const THREE = __noop;\n");
const fb = Deno.readTextFileSync(`${dir}/web/ui/finbuild.js`).replace("import * as THREE from 'three';", "import { THREE } from './threestub.js';");
Deno.writeTextFileSync(`${dir}/web/ui/finbuild.js`, fb);
globalThis.window = { addEventListener() {} };
globalThis.document = { querySelectorAll: () => [], getElementById: () => null };
globalThis.localStorage = { getItem: () => null, setItem() {} };
globalThis.setTimeout = () => 0; globalThis.clearTimeout = () => {};
const root = `file://${dir}/web/`;
const domMod = await import(`${root}ui/dom.js`);
const settingsMod = await import(`${root}ui/settings.js`);
const finbuildMod = await import(`${root}ui/finbuild.js`);
const { FIN, PAD } = await import(`${root}fins.js`);
const PAD_INITIAL = structuredClone({ style: PAD.style, custom: PAD.custom });

/** finOpts() of the page with the controls set for `form` (a FinSettings in the app's field names). */
function pageFinOpts(form) {
  const f = { ...DEFAULTS, ...form };
  domMod.reset();
  PAD.style = PAD_INITIAL.style; Object.assign(PAD.custom, PAD_INITIAL.custom);
  const set = (id, v) => { const c = domMod.el(id); typeof v === "boolean" ? (c.checked = v) : (c.value = String(v)); };
  settingsMod.setFinMode(f.mode);
  set("nozzle", f.nozzle); set("layer-height", f.layerHeight); set("gap", f.gap);
  set("iface", f.interfaceMaterial); set("tines", f.tines); set("tine-density", f.tineDensity);
  set("tine-dir", f.tineDirection); set("pin-depth", f.pinDepth); set("cutout", f.cutout);
  set("rounded-feet", f.roundedFeet); set("bed-pad", f.bedPad);
  set("wall-lines", f.wallLines); set("tall-ribs", f.tallRibs); set("tall-ribs-height", f.tallRibsHeight);
  set("sway", f.sway); set("sway-from", f.swayFrom); set("sway-spacing", f.swaySpacing); set("sway-depth", f.swayDepth);
  set("full-brace-depth", f.fullBraceDepth); set("sway-cross-height", f.swayCrossHeight);
  set("coverage", f.coverage); set("plate-only", f.plateOnly); set("separate-walls", f.separateWalls);
  set("long-walls", f.longWalls); set("curved-walls", f.curvedWalls);
  const pads = { "pad-h": f.padThickness, "pad-gap": f.padGap, "pad-grip": f.padGrip, "pad-margin": f.padSpread };
  settingsMod.initSettings();
  // picking Custom fills the four fields from a preset; the user then types their own, field by field
  if (f.bedPad === "custom") for (const [id, v] of Object.entries(pads)) { set(id, v); domMod.fire(id, "input"); }
  return JSON.parse(JSON.stringify(finbuildMod.finOpts([])));
}

const EXTRAS = ["tallRibs", "curvedWalls", "sway.uncappedDepth", "sway.crossRise"];

const forms = {
  defaults: {},
  nozzle06: { nozzle: 0.6 }, nozzle25: { nozzle: 0.25 }, nozzle20: { nozzle: 2.0 },
  layer: { layerHeight: 0.12 }, gap: { gap: 0.3 }, threshold: { threshold: 60 },
  iface: { interfaceMaterial: "all" },
  full: { mode: "full" }, fullWithSlider: { mode: "full", coverage: 80 }, draw: { mode: "draw", sway: true },
  tinesOff: { tines: false }, density: { tineDensity: 35 }, densityMax: { tineDensity: 100 },
  vertical: { tineDirection: "vertical", pinDepth: 0.25, separateWalls: true },
  verticalUnticked: { tineDirection: "vertical", separateWalls: false },
  cutouts: { cutout: "honeycomb" },
  roundedFeet: { roundedFeet: true },
  padOff: { bedPad: "off" }, padLight: { bedPad: "light" }, padSure: { bedPad: "sure" },
  padCustom: { bedPad: "custom", padThickness: 0.5, padGap: 0, padGrip: 0.05, padSpread: 8 },
  wallLines: { wallLines: 6 },
  noRibs: { tallRibs: false, tallRibsHeight: 25 },
  sway: { sway: true },
  swayAll: { sway: true, swayFrom: 40, swaySpacing: 10, swayDepth: 30, fullBraceDepth: true, swayCrossHeight: 70 },
  swayFull: { sway: true, mode: "full", swayDepth: 5 },
  coverageLow: { coverage: 0 }, coverageHigh: { coverage: 100 }, coverage35: { coverage: 35 },
  plateOnly: { plateOnly: true },
  separate: { separateWalls: true, mode: "full" },
  shortWalls: { longWalls: false, mode: "full" },
  curved: { curvedWalls: true },
  display: { highlightSmall: false, showIslands: true },
  tunableEdges: { tallRibsHeight: 500, bedPad: "custom", padThickness: 5, padGap: -1, padGrip: 1, padSpread: 40, swayCrossHeight: 5, sway: true },
  everything: {
    mode: "full", threshold: 38, nozzle: 0.8, layerHeight: 0.28, gap: 0.15, interfaceMaterial: "flat", tines: true,
    tineDensity: 60, tineDirection: "vertical", pinDepth: 0.4, cutout: "lattice", roundedFeet: true, bedPad: "sure",
    wallLines: 4, tallRibs: false, sway: true, swayFrom: 15, swaySpacing: 4, swayDepth: 22, coverage: 20,
    plateOnly: true, separateWalls: true, longWalls: false, curvedWalls: true, highlightSmall: false, showIslands: true,
  },
};

const cases = Object.entries(forms).map(([id, form]) => {
  const dialog = dialogOf(form);
  const engineDialog = Object.fromEntries(Object.entries(dialog).filter(([k]) => !EXTRAS.includes(k)));
  const page = pageFinOpts(form);
  // the restated dialog must agree with what the page's own finOpts() asks for
  const want = { ...(page.mode !== "draw" ? { mode: page.mode } : {}), tines: page.tines, tineDensity: page.tineDensity,
    layerHeight: page.layerHeight, coverage: page.coverage, plateOnly: page.plateOnly, longWalls: page.longWalls,
    separateWalls: page.separateWalls ? "on" : "off", tallRibs: page.tallRibs, curvedWalls: page.curvedWalls === true,
    "sway.on": !!page.sway, ...(page.sway ? { "sway.gripFrom": page.sway.gripFrom, "sway.tineSpacing": page.sway.tineSpacing,
      "sway.reach": page.sway.reach, "sway.uncappedDepth": page.sway.uncappedDepth, "sway.crossRise": page.sway.crossRise } : {}) };
  for (const [k, v] of Object.entries(want)) {
    if (dialog[k] !== v) throw new Error(`${id}: dialog ${k} = ${dialog[k]}, the page's finOpts() has ${v}`);
  }
  return { id, args: [form], out: { dialog, options: optionsFromDialog(engineDialog), finOpts: page } };
});
writeCases("settings", "optionsJSON", cases);
Deno.removeSync(dir, { recursive: true });
console.log("settings:", cases.length, "cases");
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
