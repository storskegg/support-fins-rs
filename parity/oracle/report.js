// Records parity/corpus/report/*.jsonl (web/report.js; direct cases).
//   reportPart      args: [pos (Float64Array soup), scale]          out: Float32Array
//   reportSettings  args: [{threshold, rot, opts, scale, size, faces}]   out: the settings JSON
//   reportCoverage  args: [built, res]   built: {fins: [{}..], braceCount, propCount, tines, sway: {count}?, bare, fill,
//                   unserved, smallHoleRegions, floating: [{}..], pad: {style}|null}; res: {regions: [{}..]}
//                   out: the coverage object (keys with undefined values are absent)
//   reportUrl       args: [rep, cov, {where, when, drawn, removed}]  out: the link (string)
//   replayReport    args: [pos (Float64Array soup), rep (JSON text of the settings), {$config: <every config object>}]
//                   out: {why} | {nTriangles, hash, nPad, padHash, nFins, coverage}
// Replayed by crates/sf-io/tests/parity_report.rs.
import { writeCases } from "./lib/encode.js";
import { modelList } from "./lib/direct.js";
import { WEB } from "./lib/pin.js";

const { reportPart, reportSettings, reportCoverage, reportUrl, replayReport } = await import(`${WEB}report.js`);
const { buildTopology, analyze } = await import(`${WEB}overhangs.js`);
const { buildFins } = await import(`${WEB}fins.js`);
const { Ref, encode: enc } = await import("./lib/encode.js");
const cfgMods = {};
for (const [k, p] of Object.entries({
  prop: "prop/config.js", fin: "fins/config.js", pad: "fins/pad.js", perp: "fins/wedges.js", sway: "sway.js", cut: "cutout.js",
  island: "islands.js", trace: "prop/trace.js", tie: "prop/ties.js", brace: "prop/brace.js", resin: "prop/resinfoot.js",
  judge: "prop/judge.js", downhill: "prop/downhill.js", field: "fins/field.js", curved: "prop/curved.js", rim: "prop/rim.js",
  arc: "prop/arc.js", fill: "fins/fill.js", rows: "fins/rows.js", nozzle: "nozzle.js",
})) cfgMods[k] = await import(WEB + p);
const m = cfgMods;
const snap = () => ({
  PROP: m.prop.PROP, FIN: m.fin.FIN, PAD: m.pad.PAD, PERP: m.perp.PERP, SWAY: m.sway.SWAY, CUT: m.cut.CUT, ISLAND: m.island.ISLAND,
  TRACE: m.trace.TRACE, TIE: m.tie.TIE, BRACE: m.brace.BRACE, RESIN_FOOT: m.resin.RESIN_FOOT, JUDGE: m.judge.JUDGE,
  DOWNHILL: m.downhill.DOWNHILL, FIELD: m.field.FIELD, CURVED: m.curved.CURVED, RIM: m.rim.RIM, ARC: m.arc.ARC, FILL: m.fill.FILL,
  ROWS: m.rows.ROWS, NOZZLE: { nozzle: m.nozzle.nozzle, wallLines: m.nozzle.wallLines },
});
const { readSTL } = await import(`${WEB}stl.js`);

const cases = { reportPart: [], reportSettings: [], reportCoverage: [], reportUrl: [], replayReport: [] };

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

// replayReport: a page-like build filed as JSON, replayed from the file's triangles; and the refusals
{
  const { siteOpts } = await import(`${WEB}../tests/_scene.js`);
  const hashOf = (flat) => {
    const w = new Uint32Array(flat.buffer);
    let h1 = 0x811c9dc5, h2 = 0x9747b28c;
    for (let i = 0; i < w.length; i++) { h1 = Math.imul(h1 ^ w[i], 16777619); h2 = Math.imul(h2 ^ w[i], 0x01000193) ^ (h2 >>> 13); }
    return [h1 >>> 0, h2 >>> 0];
  };
  const flat = (tris) => { const f = new Float64Array(tris.length * 4); tris.forEach((v, i) => { f[i * 4] = v[0]; f[i * 4 + 1] = v[1]; f[i * 4 + 2] = v[2]; f[i * 4 + 3] = v[3] === 1 ? 1 : 0; }); return f; };
  const cfgRef = () => new Ref({ $config: enc(snap()) });
  const out = (r) => (r.why ? { why: r.why } : { nTriangles: r.built.triangles.length, hash: hashOf(flat(r.built.triangles)), nPad: r.built.padTriangles.length,
    padHash: hashOf(flat(r.built.padTriangles)), nFins: r.built.fins.length, coverage: reportCoverage(r.built, r.res) });
  const rotOf = (ax, az) => { const [c, s, C, S] = [Math.cos(ax), Math.sin(ax), Math.cos(az), Math.sin(az)];
    const mm = [[C, -S * c, S * s], [S, C * c, -C * s], [0, s, c]]; return [mm[0][0], mm[1][0], mm[2][0], mm[0][1], mm[1][1], mm[2][1], mm[0][2], mm[1][2], mm[2][2]]; };
  for (const name of ["cube", "wedge", "lowledge", "portal", "lbracket"]) {
    const path = modelList().find(([n]) => n === name)[1];
    const raw = readSTL(Deno.readFileSync(path));
    for (const scale of [1, 1.5]) for (const mode of ["auto", "full", "prop"]) {
      const rot = rotOf(Math.PI / 6, Math.PI / 9);
      const p = reportPart(raw, scale);
      const topo = buildTopology({ getAttribute: (k) => (k === "position" ? { array: p } : null) });
      const res = analyze(topo, 45, rot);
      const opts = siteOpts({ material: "pla", mode, sway: false });
      let lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
      for (let i = 0; i < p.length; i += 3) for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], p[i + k]); hi[k] = Math.max(hi[k], p[i + k]); }
      const rep = JSON.parse(JSON.stringify(reportSettings({ threshold: 45, rot, opts, scale, size: lo.map((v, k) => hi[k] - v), faces: p.length / 9 })));
      const id = `${name}|s${scale}|${mode}`;
      cases.replayReport.push({ id, args: [raw, JSON.stringify(rep), cfgRef()], out: out(replayReport(raw, rep)) });
      if (mode === "auto") {
        for (const [tag, bad] of [["scale", { ...rep, scale: scale === 1 ? 2 : 1 }], ["version", { ...rep, v: 99 }], ["faces", { ...rep, part: { ...rep.part, faces: rep.part.faces + 1 } }], ["nover", { opts: rep.opts }]]) {
          cases.replayReport.push({ id: `${id}|${tag}`, args: [raw, JSON.stringify(bad), cfgRef()], out: out(replayReport(raw, bad)) });
        }
      }
    }
  }
}

for (const [fn, cs] of Object.entries(cases)) writeCases("report", fn, cs);
console.log("report:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])));
