// Records parity/corpus/report/*.jsonl (web/report.js; direct cases).
//   reportPart      args: [pos (Float64Array soup), scale]          out: Float32Array
//   reportSettings  args: [{threshold, rot, opts, scale, size, faces}]   out: the settings JSON
//   reportCoverage  args: [built, res]   built: {fins: [{}..], braceCount, propCount, tines, sway: {count}?, bare, fill,
//                   unserved, smallHoleRegions, floating: [{}..], pad: {style}|null}; res: {regions: [{}..]}
//                   out: the coverage object (keys with undefined values are absent)
//   reportUrl       args: [rep, cov, {where, when, drawn, removed}]  out: the link (string)
// replayReport needs buildFins and is recorded with it. Replayed by crates/sf-testkit/tests/parity_report.rs.
import { writeCases } from "./lib/encode.js";
import { modelList } from "./lib/direct.js";
import { WEB } from "./lib/pin.js";

const { reportPart, reportSettings, reportCoverage, reportUrl } = await import(`${WEB}report.js`);
const { readSTL } = await import(`${WEB}stl.js`);

const cases = { reportPart: [], reportSettings: [], reportCoverage: [], reportUrl: [] };

for (const [name, path] of modelList()) {
  const pos = readSTL(Deno.readFileSync(path));
  if (pos.length > 3 * 9 * 4000) continue;
  for (const scale of [1, 1.5, 0.5, 2.37, 0.1]) cases.reportPart.push({ id: `${name}|s${scale}`, args: [pos, scale], out: reportPart(pos, scale) });
}

const ROT = [0.8660254037844387, 0, -0.5, 0, 1, 0, 0.5, 0, 0.8660254037844387];
const optsA = { mode: "auto", bedPad: true, plateOnly: false, tines: true, tineDensity: 0.6, layerHeight: 0.2, coverage: 0.7, sway: undefined, curvedWalls: undefined,
  tunables: { roundFeet: true, propGap: 0.25, padStyle: "sure", padCustom: { h: 0.5, gap: 0, grip: 0.05, margin: 4 } } };
const optsF = { ...optsA, mode: "full", coverage: 0.5, islands: [3, 17, 42], sway: { on: true, gripFrom: 0, tineSpacing: 6 } };
const optsD = { mode: "draw", tines: false, nan: NaN, inf: Infinity, nested: { a: [1, undefined, 3], b: undefined } };
let k = 0;
for (const opts of [optsA, optsF, optsD, { mode: "prop" }, {}]) {
  for (const [scale, size, faces] of [[1, [10.1234567, 20, 30.0000004], 12], [1.5, [15.5, 0.0000004, 4.5], 1200], [0.3333333, [1, 2, 3], 7]]) {
    const a = { threshold: 45, rot: ROT, opts, scale, size, faces };
    cases.reportSettings.push({ id: `${k++}|${opts.mode}|s${scale}`, args: [a], out: reportSettings(a) });
  }
}

const mkBuilt = (o = {}) => ({ fins: Array.from({ length: o.fins ?? 5 }, () => ({})), braceCount: o.braceCount, propCount: o.propCount, tines: o.tines ?? 12,
  sway: o.sway, bare: o.bare, fill: o.fill, unserved: o.unserved ?? 0, smallHoleRegions: o.smallHoleRegions ?? 0,
  floating: Array.from({ length: o.floating ?? 0 }, () => ({})), pad: o.pad ?? null });
const builtList = [
  mkBuilt(),
  mkBuilt({ braceCount: 4, propCount: 0, bare: { area: 12.4, total: 80.6, coverage: 0.5 }, unserved: 2, smallHoleRegions: 1, floating: 2, pad: { style: "sure" } }),
  mkBuilt({ bare: { area: 0, total: 0, coverage: 0.5 }, sway: { count: 3 } }),
  mkBuilt({ bare: { area: NaN, total: 10, coverage: 0.5 }, pad: { style: "light" } }),
  mkBuilt({ bare: { area: 33.3333, total: 99.9999, coverage: 0.5 }, fill: { walls: 7, unservedArea: 4.5, capped: true, islandsBare: 2 }, propCount: 1, braceCount: 6, pad: { style: "custom" } }),
  mkBuilt({ fill: { walls: 0, unservedArea: Infinity, capped: false, islandsBare: 0 }, fins: 0, bare: { area: 5, total: 0 }, unserved: NaN }),
];
builtList.forEach((b, i) => cases.reportCoverage.push({ id: `b${i}`, args: [b, { regions: Array.from({ length: i + 1 }, () => ({})) }], out: reportCoverage(b, { regions: Array.from({ length: i + 1 }, () => ({})) }) }));

const covs = builtList.map((b, i) => reportCoverage(b, { regions: Array.from({ length: i + 1 }, () => ({})) }));
const reps = [];
for (const opts of [optsA, optsF, optsD, { mode: "prop" }, { mode: "weird & mode" }, {}]) {
  for (const scale of [1, 1.5, 0.3333333]) {
    reps.push(reportSettings({ threshold: 45, rot: ROT, opts, scale, size: [10.12, 20, 30], faces: 12 }));
  }
}
const urlOpts = [{ where: "printfins.com", when: "2026-10-27" }, { where: "preview/üñí branch", when: "2026-10-27", drawn: 2, removed: 3 }];
let j = 0;
for (const rep of reps) for (const [ci, cov] of covs.entries()) for (const o of urlOpts) {
  if ((j++) % 3) continue;
  cases.reportUrl.push({ id: `${j}|${rep.opts.mode}|c${ci}|${o.drawn ?? 0}`, args: [rep, cov, o], out: reportUrl(rep, cov, o) });
}
// a request too big for a link keeps the summary and points at Copy settings
const big = { ...reps[1], opts: { ...reps[1].opts, pad: "x".repeat(9000) } };
cases.reportUrl.push({ id: "too-long", args: [big, covs[1], urlOpts[0]], out: reportUrl(big, covs[1], urlOpts[0]) });
const near = { ...reps[0], opts: { ...reps[0].opts, pad: "é ".repeat(560) } };
cases.reportUrl.push({ id: "near-limit", args: [near, covs[0], urlOpts[0]], out: reportUrl(near, covs[0], urlOpts[0]) });

for (const [fn, cs] of Object.entries(cases)) writeCases("report", fn, cs);
console.log("report:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])));
