// Records parity/corpus/ffireport/cases.json: "Something didn't get supported?" and Copy settings as the page
// makes them -- the `finOpts()` request (ui/finbuild.js), `buildFins`, then `reportSettings`, `reportCoverage` and
// `reportUrl` (web/report.js) -- for the builds of parity/corpus/ffibuild/cases.json. The app sends its build as
// two texts (`opts_json` + `tunables_json`, docs/FFI.md); crates/sf-ffi/tests/report.rs checks that
// `build_report(result, scale, opts_json, tunables_json)` gives the same settings JSON (as Copy settings writes
// it, `JSON.stringify(rep, null, 2)`) and the same link, byte for byte.
//
// A case: { name, part, rot, scale, options, tunables, urls: [{ site, when, drawn, removed, url }], settings }.
//   options   what the app sends as opts_json (the keys FinSettings.optionsJSON writes)
//
//   just oracle ffireport
import { UPSTREAM_ROOT, assertPinned } from "./lib/pin.js";

assertPinned();
const WEB = `${UPSTREAM_ROOT}/web`;
const { buildTopology, analyze } = await import(`${WEB}/overhangs.js`);
const { FIN, buildFins } = await import(`${WEB}/fins.js`);
const { readSTL } = await import(`${WEB}/stl.js`);
const { reportSettings, reportCoverage, reportUrl } = await import(`${WEB}/report.js`);

const base = JSON.parse(Deno.readTextFileSync(new URL("../corpus/ffibuild/cases.json", import.meta.url)));

function block(x0, x1, y0, y1, z0, z1) {
  const v = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
  const quad = (a, b, c, d) => [...a, ...b, ...c, ...a, ...c, ...d];
  return Float32Array.from([...quad(v[0], v[3], v[2], v[1]), ...quad(v[4], v[5], v[6], v[7]), ...quad(v[0], v[1], v[5], v[4]),
    ...quad(v[2], v[3], v[7], v[6]), ...quad(v[1], v[2], v[6], v[5]), ...quad(v[0], v[4], v[7], v[3])]);
}
const topoOf = (pos) => buildTopology({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });

// the page's request, from the settings the app sends (the DOM reads of ui/finbuild.js finOpts / swayOpts)
function pageRequest(o, tunables) {
  const sway = o["sway.on"];
  return {
    mode: o.mode,
    bedPad: (o.padStyle ?? "auto") !== "off",
    plateOnly: o.plateOnly,
    longWalls: o.longWalls,
    separateWalls: o.separateWalls === "on",
    tallRibs: o.tallRibs,
    curvedWalls: o.curvedWalls ? true : undefined,
    tines: o.tines,
    tineDensity: o.tineDensity,
    layerHeight: o.layerHeight,
    coverage: o.mode === "full" ? FIN.coverDefault : o.coverage,
    sway: sway ? { on: true, roundFeet: tunables.roundFeet, uncappedDepth: o["sway.uncappedDepth"], gripFrom: o["sway.gripFrom"],
      tineSpacing: o["sway.tineSpacing"], reach: o["sway.reach"], crossRise: o["sway.crossRise"], gap: tunables.propGap,
      tines: o.tines, iface: tunables.iface, layerHeight: o.layerHeight,
      crossAbove: o.tallRibs ? tunables.tallRibsHeight : Infinity } : undefined,
    tunables,
  };
}

const out = [];
let i = 0;
for (const c of base.cases) {
  const pos = c.part.model ? readSTL(Deno.readFileSync(`${UPSTREAM_ROOT}/prototype/stress/models/${c.part.model}.stl`)) : block(...c.part.block);
  const d = c.dialog;
  const options = {
    mode: d.mode, threshold: 45, tines: true, tineDensity: d.tineDensity, layerHeight: d.layerHeight,
    plateOnly: false, separateWalls: "off", longWalls: true, tallRibs: d.tallRibs ?? true,
    coverage: d.mode === "full" ? 0.5 : d.coverage, curvedWalls: d.curvedWalls ?? false, "sway.on": d["sway.on"] ?? false,
  };
  if (options["sway.on"]) for (const k of ["sway.gripFrom", "sway.tineSpacing", "sway.reach", "sway.uncappedDepth", "sway.crossRise"]) options[k] = d[k];
  const topo = topoOf(pos);
  const res = analyze(topo, 45, c.rot);
  const built = buildFins(topo, res, c.rot, pageRequest(options, c.tunables));
  const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
  for (let j = 0; j < pos.length; j += 3) for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], pos[j + k]); hi[k] = Math.max(hi[k], pos[j + k]); }
  const scale = i % 2 ? 1.5 : 1;
  const rep = reportSettings({ threshold: 45, rot: c.rot, opts: pageRequest(options, c.tunables), scale, size: hi.map((v, k) => v - lo[k]), faces: pos.length / 9 });
  const cov = reportCoverage(built, res);
  const urls = [
    { site: "Support Fins for Mac 0.1.0 (upstream 299ef49)", when: "2026-10-28", drawn: 0, removed: 0 },
    { site: "printfins.com", when: "2026-10-27", drawn: 3, removed: 2 },
  ].map((u) => ({ ...u, url: reportUrl(rep, cov, { where: u.site, when: u.when, drawn: u.drawn, removed: u.removed }) }));
  out.push({ name: c.name, part: c.part, rot: c.rot, scale, options, tunables: c.tunables, settings: JSON.stringify(rep, null, 2), urls });
  console.log(`${c.name}: ${urls[0].url.length} chars`);
  i++;
}
Deno.mkdirSync(new URL("../corpus/ffireport", import.meta.url), { recursive: true });
Deno.writeTextFileSync(new URL("../corpus/ffireport/cases.json", import.meta.url), JSON.stringify({ upstream: base.upstream, cases: out }));
