// Records parity/corpus/search/cases.json: the settings search (web/ui/search.js) run unmodified on the real
// options panel of web/index.html (parsed with linkedom, a dev-only dependency of this recorder), for a set of
// window states and queries. app/SupportFinsTests/SettingsSearchTests.swift replays them through SearchEngine.
//
// A case: { id, steps: [{ state, query, hits, nohit, empty }] }. The steps of one case run in order on one page,
// the way a user types and changes a setting (the page keeps what a query revealed while the query stays).
//   state   what the window looked like: { settings: <FinSettings fields that differ from the defaults>, ui: {...} }
//           ui: augment / remove / restore (the buttons Draw and Remove fins offer), draw (its controls),
//           hint / started / selected (Draw's Line-Curve, Back one point, Remove selected)
//   hits    the ids of the rows the page marked `.hit` (the app's row ids; Line and Curve are one row, draw-shape)
//   nohit   the blocks the page hid (`.nohit`): printer, placement, tines, more, draw, holdThePart, tallParts, whereSupportsGo, display
//   empty   "No settings match" showing
// The `hidden` flags the page's ui/settings.js, ui/remove.js and ui/walls.js set for a state are applied here by
// the same rules (settings.js:40-70, 165-225, 265; remove.js:106-113; walls.js:602-612), since those modules need
// the whole page.
//
//   just oracle search
import { assertPinned, UPSTREAM_ROOT } from "./lib/pin.js";
import { parseHTML } from "npm:linkedom@0.18.5";

assertPinned();
const html = Deno.readTextFileSync(`${UPSTREAM_ROOT}/web/index.html`);
const { document, window } = parseHTML(html);
Object.assign(globalThis, { document, window, location: { search: "", host: "" }, requestAnimationFrame: (f) => f(),
  MutationObserver: class { observe() {} }, localStorage: { getItem: () => null, setItem() {} } });
document.getElementById("fin-opts").hidden = false;

const search = await import(`file://${UPSTREAM_ROOT}/web/ui/search.js`);
const $ = (id) => document.getElementById(id);
const label = (id) => $(id).closest("label");

// the app's row id -> the page's row element
const ROW = {
  "nozzle-fld": () => $("nozzle-fld"), "layerh-fld": () => $("layerh-fld"), gap: () => label("gap"), iface: () => label("iface"),
  "fin-mode-fld": () => $("fin-mode-fld"), tines: () => label("tines"), "tinegrip-fld": () => $("tinegrip-fld"),
  "tinedir-fld": () => $("tinedir-fld"), "pindepth-fld": () => $("pindepth-fld"), cutout: () => label("cutout"),
  "augment-toggle": () => $("augment-toggle"), "remove-fins-toggle": () => $("remove-fins-toggle"), "restore-fins": () => $("restore-fins"),
  "draw-line": () => document.querySelector("[data-shape=line]"), "draw-curve": () => document.querySelector("[data-shape=curve]"),
  "draw-back": () => $("draw-back"), "draw-remove": () => $("draw-remove"), "draw-clear": () => $("draw-clear"),
  "rounded-feet-fld": () => $("rounded-feet-fld"), "bed-pad": () => label("bed-pad"), "pad-h": () => label("pad-h"),
  "pad-gap": () => label("pad-gap"), "pad-grip": () => label("pad-grip"), "pad-margin": () => label("pad-margin"),
  "wall-lines": () => label("wall-lines"), "tall-ribs": () => label("tall-ribs"), "tall-ribs-height-fld": () => $("tall-ribs-height-fld"),
  sway: () => label("sway"), "sway-from-fld": () => $("sway-from-fld"), "sway-spacing-fld": () => $("sway-spacing-fld"),
  "sway-depth-fld": () => $("sway-depth-fld"), "full-brace-depth-fld": () => $("full-brace-depth-fld"),
  "sway-cross-height-fld": () => $("sway-cross-height-fld"), "coverage-fld": () => $("coverage-fld"), "plate-only": () => label("plate-only"),
  "separate-walls-fld": () => $("separate-walls-fld"), "long-walls-fld": () => $("long-walls-fld"), "curved-walls-fld": () => $("curved-walls-fld"),
  "highlight-small": () => label("highlight-small"), "show-islands": () => label("show-islands"),
};
const BLOCK = {
  printer: () => document.querySelector("details[data-sec=printer]"), placement: () => document.querySelector(".sec.placement"),
  tines: () => document.querySelector("details[data-sec=tines]"), more: () => $("more-settings"), draw: () => $("draw-controls"),
  holdThePart: () => document.querySelectorAll("#more-settings .group")[0], tallParts: () => document.querySelectorAll("#more-settings .group")[1],
  whereSupportsGo: () => document.querySelectorAll("#more-settings .group")[2], display: () => document.querySelectorAll("#more-settings .group")[3],
};

// every row of the panel is one of ours, and every one of ours is a row of the panel
{
  const pane = $("fin-opts"), ROWS = ".fld, label.check, button.btn:not([data-mode])";
  const rows = [...pane.querySelectorAll(ROWS)].filter((r) => !r.closest(".opts-search") && !r.parentElement.closest(ROWS));
  const ours = new Set(Object.values(ROW).map((f) => f()));
  for (const r of rows) if (!ours.has(r)) throw new Error(`a row of the page the catalog lacks: ${r.id || r.textContent.trim()}`);
  for (const [id, f] of Object.entries(ROW)) if (!rows.includes(f())) throw new Error(`${id} is not a row of the page`);
}

// linkedom's <select>.value has only a getter
const setValue = (id, v) => Object.defineProperty($(id), "value", { get: () => v, configurable: true });

// The two rows whose title ui/settings.js swaps (syncTineGrip, syncCoverageUI) keep the first one in
// dataset.origTitle, which search.js reads beside the title. The constants are read from settings.js itself.
const settingsSrc = Deno.readTextFileSync(`${UPSTREAM_ROOT}/web/ui/settings.js`);
const constant = (name) => new Function(`return ${settingsSrc.match(new RegExp(`const ${name} = ([\\s\\S]+?);\\n`))[1]}`)();
const PIN_SPACING_TIP = constant("PIN_SPACING_TIP"), COVER_TIP_FULL = constant("COVER_TIP_FULL");
const GRIP_TIP = $("tinegrip-fld").title, COVER_TIP = $("coverage-fld").title;

const DEFAULTS = { mode: "auto", tines: true, tineDirection: "horizontal", bedPad: "auto", interfaceMaterial: "off", sway: false, tallRibs: true, curvedWalls: false };

/** Put the page in the state the window would be in (the `hidden` rules of ui/settings.js, remove.js, walls.js). */
function apply({ settings = {}, ui = {} }) {
  const s = { ...DEFAULTS, ...settings };
  const autoLike = s.mode !== "draw", full = s.mode === "full";
  setValue("fin-mode", s.mode);
  $("tines").checked = s.tines;
  setValue("tine-dir", s.tineDirection);
  setValue("bed-pad", s.bedPad);
  setValue("iface", s.interfaceMaterial);
  $("sway").checked = s.sway;
  $("tall-ribs").checked = s.tallRibs;
  $("curved-walls").checked = s.curvedWalls;
  // syncCoverageUI
  $("coverage-fld").hidden = !autoLike;
  $("long-walls-fld").hidden = !full;
  $("separate-walls-fld").hidden = !(full || (autoLike && s.curvedWalls));
  $("curved-walls-fld").hidden = !autoLike;
  // syncTineGrip
  const on = s.tines, pins = on && s.tineDirection === "vertical";
  $("rounded-feet-fld").hidden = s.tineDirection === "vertical";
  $("tinegrip-fld").hidden = !on;
  $("tinedir-fld").hidden = !on;
  $("tinegrip-fld").querySelector(".grip-tines").hidden = pins;
  $("tinegrip-fld").querySelector(".grip-pins").hidden = !pins;
  $("pindepth-fld").hidden = !pins;
  $("tinegrip-fld").dataset.origTitle = GRIP_TIP;
  $("tinegrip-fld").title = pins ? PIN_SPACING_TIP : GRIP_TIP;
  $("coverage-fld").dataset.origTitle = COVER_TIP;
  $("coverage-fld").title = full ? COVER_TIP_FULL : COVER_TIP;
  $("layerh-fld").hidden = (!on || pins) && !["light", "auto"].includes(s.bedPad) && s.interfaceMaterial === "off";
  // the pad's numbers
  for (const f of document.querySelectorAll("[data-pad-custom]")) f.hidden = s.bedPad !== "custom";
  // syncSway, the tall-ribs height
  $("sway-from-fld").hidden = !s.sway || !s.tines;
  $("sway-spacing-fld").hidden = !s.sway || !s.tines;
  $("sway-depth-fld").hidden = !s.sway;
  $("full-brace-depth-fld").hidden = !s.sway;
  $("sway-cross-height-fld").hidden = !s.sway || !s.tallRibs;
  $("tall-ribs-height-fld").hidden = !s.tallRibs;
  // Add walls by hand (syncAugmentUI), Remove fins (syncRemoveUI), Draw's controls (syncDrawControls)
  $("augment-toggle").hidden = !ui.augment;
  $("remove-fins-controls").hidden = !ui.remove;
  $("remove-fins-toggle").hidden = !ui.remove;
  $("restore-fins").hidden = !(ui.remove && ui.restore);
  $("draw-controls").hidden = !ui.draw;
  $("draw-hint").hidden = $("draw-shape").hidden = !ui.hint;
  $("draw-back").hidden = !ui.started;
  $("draw-remove").hidden = !ui.selected;
}

function record(query) {
  const box = $("opts-search");
  box.value = query;
  search.runSearch();
  const pane = $("fin-opts");
  const hit = (id) => ROW[id]().classList.contains("hit");
  const hits = Object.keys(ROW).filter((id) => id !== "draw-line" && id !== "draw-curve" && hit(id));
  if (hit("draw-line") || hit("draw-curve")) hits.push("draw-shape");
  const nohit = Object.keys(BLOCK).filter((b) => BLOCK[b]().classList.contains("nohit"));
  return { hits, nohit, empty: !$("opts-search-empty").hidden, searching: pane.classList.contains("searching") };
}
function clear() { $("opts-search").value = ""; search.runSearch(); }

const STATES = {
  default: {},
  "tines off": { settings: { tines: false } },
  "tines off, pad off, iface off": { settings: { tines: false, bedPad: "off" } },
  vertical: { settings: { tineDirection: "vertical" } },
  sway: { settings: { sway: true } },
  "sway, tines off": { settings: { sway: true, tines: false } },
  "sway, tall ribs off": { settings: { sway: true, tallRibs: false } },
  "tall ribs off": { settings: { tallRibs: false } },
  "pad custom": { settings: { bedPad: "custom" } },
  full: { settings: { mode: "full" } },
  "full, curved": { settings: { mode: "full", curvedWalls: true } },
  curved: { settings: { curvedWalls: true } },
  "iface flat": { settings: { interfaceMaterial: "flat", tines: false, bedPad: "off" } },
  draw: { settings: { mode: "draw" }, ui: { draw: true } },
  "draw, placing": { settings: { mode: "draw" }, ui: { draw: true, hint: true } },
  "draw, started, selected": { settings: { mode: "draw" }, ui: { draw: true, hint: true, started: true, selected: true } },
  "auto, augment": { ui: { augment: true } },
  "auto, remove": { ui: { augment: true, remove: true } },
  "auto, remove, restore": { ui: { augment: true, remove: true, restore: true } },
};
const QUERIES = ["gap", "tine grip", "light", "diamond", "flat", "sure", "brace depth", "brace", "pad", "pad gap", "mm", "layer", "coverage",
  "clear all", "clear", "remove", "restore", "sway", "tall", "cutouts", "nozzle", "wall", "wall lines", "bridging", "clearance", "amber", "zzz",
  "TINES", "tine  grip", "sepárate", "SEPARATE", "pins", "pin depth", "draw", "line", "curve", "back", "selected", "thick", "0.4", "hold",
  "firm", "dense", "plate", "walls", "curved", "midair", "small", "interface", "everywhere", "off", "custom", "arch", "honeycomb", "brace tine spacing",
  "cross-rib", "cross rib", "above", "% of height", "mm up", "grip from", "full", "placement", "auto", "add walls", "by hand", "cm", "x", " ", "e", "anti-sag", "tightly", "firmer", "applies in auto"];

const cases = [];
for (const [name, state] of Object.entries(STATES)) {
  for (const q of QUERIES) {
    apply(state);
    cases.push({ id: `${name}|${JSON.stringify(q)}`, steps: [{ state, query: q, ...record(q) }] });
    clear();
  }
}

// what a query revealed stays while the query is the same (the switch to turn off again), and goes with a new query
const sequences = [
  ["brace depth", [{ settings: {} }, { settings: { sway: true } }, { settings: { sway: true } }], ["brace depth", "brace depth", "brace d"]],
  ["pad gap", [{ settings: {} }, { settings: { bedPad: "custom" } }, { settings: {} }], ["pad gap", "pad gap", "pad gap"]],
  ["layer height", [{ settings: { tines: false, bedPad: "off" } }, { settings: { tines: true, bedPad: "off" } }, { settings: { tines: false, bedPad: "off" } }],
    ["layer height", "layer height", "layer height"]],
  ["clear all", [{ settings: {} }, { settings: { mode: "draw" }, ui: { draw: true, hint: true } }, { settings: { mode: "draw" }, ui: { draw: true, hint: true } }],
    ["clear all", "clear all", "clear"]],
  ["restore", [{ ui: {} }, { ui: { augment: true, remove: true, restore: true } }, { ui: { augment: true, remove: true } }], ["restore", "restore", "restore"]],
];
for (const [name, states, queries] of sequences) {
  const steps = states.map((state, i) => { apply(state); return { state, query: queries[i], ...record(queries[i]) }; });
  clear();
  cases.push({ id: `sequence|${name}`, steps });
}

const commit = Deno.readTextFileSync(new URL("../../UPSTREAM", import.meta.url)).match(/commit=(\w+)/)[1];
Deno.mkdirSync(new URL("../corpus/search", import.meta.url), { recursive: true });
Deno.writeTextFileSync(new URL("../corpus/search/cases.json", import.meta.url), JSON.stringify({ upstream: commit, cases }));
console.log(`${cases.length} cases`);
