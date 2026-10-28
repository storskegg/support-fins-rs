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
// `tunables` is the web's finOpts().tunables (web/ui/finbuild.js:139-147) for the form: rounded feet, the pad's
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

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** finOpts().tunables (web/ui/finbuild.js:139-147) for a form, as the page's modules hold it after the clamps of
 *  ui/settings.js (gap :248, pad fields :139-142, tall ribs :267) and fins.js applyTunables (pinDepth :124).
 *  padH / padGrab are FIN.padH / PAD.grab, fixed now the PLA/PETG profile is gone; PAD.style keeps its last
 *  non-off value (a fresh page's 'auto'); PAD.custom stays the page's untouched pad.js:55 unless Custom is picked,
 *  which fills it from the four fields. */
function tunablesOf(form) {
  const f = { ...DEFAULTS, ...form };
  const custom = f.bedPad === "custom";
  return {
    roundFeet: f.roundedFeet, padH: 0.5, padGrab: 0.05,
    padStyle: f.bedPad === "off" ? "auto" : f.bedPad,
    padCustom: custom
      ? { h: clamp(f.padThickness, 0.1, 1), gap: clamp(f.padGap, 0, 0.5), grip: clamp(f.padGrip, -0.2, 0.3), margin: clamp(f.padSpread, 1, 12) }
      : { h: 0.5, gap: 0, grip: 0.05, margin: 4 },
    propGap: clamp(f.gap, 0.1, 0.4),
    tallRibsHeight: clamp(f.tallRibsHeight, 5, 200) * 10,
    cutout: f.cutout,
    iface: ({ flat: "flat", all: "all" })[f.interfaceMaterial] ?? false,
    tineDir: f.tineDirection, pinDepth: clamp(f.pinDepth, 0.05, 1),
    nozzle: f.nozzle, wallLines: f.wallLines,
  };
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
  return { id, args: [form], out: { dialog, options: optionsFromDialog(engineDialog), tunables: tunablesOf(form) } };
});
writeCases("settings", "optionsJSON", cases);
console.log("settings:", cases.length, "cases");
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
