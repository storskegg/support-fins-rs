// Records parity/corpus/fill/*.jsonl from web/fins/fill.js (and, through it, web/fins/rows.js): `fillReach`, `bareArea` and
// `fillCoverage` on real Auto builds (`direct-*`) and as the 54 golden scenes' Full builds call them (`*.captured`, named by scene).
//   fillReach      args: [coverage, {PROP}]   out: number
//   bareArea       args: [$topo, $analysis, rot, {coverage}, fins ([{kind, line}]), {PROP, FILL, ...}]   out: {area, total}
//                  (`result` is the augmented analyze result buildFins passes; the replay rebuilds it with automaticTargets)
//   fillCoverage   args: [$topo, $analysis, rot, opts, built, {config}]
//                  opts: {coverage, longWalls, fillRegions: [region index] | undefined, islands: [{z, area, points, x, y}],
//                         plateOnly, separateWalls, tines, tineDensity, layerHeight}   built: {fins: [{kind, line}], triangles: F64 (x, y, z)}
//                  out: {tris (F64, 4 per vertex), props, tines, unserved, stats}; `stats` is written with its keys sorted
//                  (JS adds them as events happen), `refused` keeps its first-seen order
import { assertPinned, WEB } from "./lib/pin.js";
import { captureModules, setScene, upstreamStatus } from "./lib/capture.js";
import { encode, Ref, writeCases } from "./lib/encode.js";
import { analyze, cfgMods, modelList, sitePose, topoOf, topoRef } from "./lib/direct.js";
import { goldenScenes } from "./lib/golden.js";

assertPinned();
const statusBefore = upstreamStatus();
const PROP = cfgMods.PROP;
const fillMod = await import(`${WEB}fins/fill.js`);
const { automaticTargets } = await import(`${WEB}fins/coverage.js`);
const { findIslands } = await import(`${WEB}islands.js`);
const finsMod = await import(`${WEB}fins.js`);
const { FILL } = fillMod;
const { ROWS } = await import(`${WEB}fins/rows.js`);
const { BRACE } = await import(`${WEB}prop/brace.js`);
const { TIE } = await import(`${WEB}prop/ties.js`);
const { FIELD } = await import(`${WEB}fins/field.js`);
const cfgNames = { PROP, FILL, ROWS, BRACE, TIE, FIELD };
const cfg = () => new Ref({ $config: Object.fromEntries(Object.entries(cfgNames).map(([k, v]) => [k, encode(v)])) });
const dec = (v) => (typeof v === "number" ? v : v.$n === "-0" ? -0 : v.$n === "NaN" ? NaN : v.$n === "Infinity" ? Infinity : -Infinity);
const flat3 = (list) => {
  const a = [];
  for (const v of list ?? []) a.push(v[0], v[1], v[2]);
  return Float64Array.from(a);
};
const flat4 = (list) => {
  const a = [];
  for (const v of list ?? []) a.push(v[0], v[1], v[2], v[3] === 1 ? 1 : 0);
  return Float64Array.from(a);
};
const slimOpts = (o) => ({
  coverage: o.coverage, longWalls: o.longWalls, fillRegions: o.fillRegions ? [...o.fillRegions] : undefined,
  islands: o.islands ? structuredClone(o.islands) : undefined, plateOnly: o.plateOnly, separateWalls: o.separateWalls,
  tines: o.tines, tineDensity: o.tineDensity, layerHeight: o.layerHeight,
});
const slimBuilt = (b) => ({ fins: (b.fins ?? []).map((f) => ({ kind: f.kind, line: f.line })), triangles: flat3(b.triangles) });
const sortKeys = (o) => Object.fromEntries(Object.entries(o).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
/** The recorded `out` of fillCoverage from the live result. */
function fillOut(f) {
  return {
    tris: flat4(f.triangles), props: f.props, tines: f.tines, unserved: f.unserved, stats: sortKeys(f.stats),
  };
}

const cases = { reach: [], bare: [], fill: [] };
for (const cov of [0, 0.1, 0.25, 0.5, 0.75, 1, 1.5, -1]) cases.reach.push({ id: `c${cov}`, args: [cov, cfg()], out: fillMod.fillReach(cov) });

let withWalls = 0, withRows = 0, withIslands = 0, withRibs = 0, withTies = 0, capped = 0;
const MODELS = ["cube", "wedge", "lbracket", "ramp", "cone", "tube", "portal", "staircase", "arch", "torus", "sphere", "lowledge", "fixtures/bowl", "fixtures/dome_ceiling", "fixtures/slot_d30_t1.6", "plus", "tshape", "ushape"];
// the models that have island starts (islands.js) at the last pose: every island option there
const ISLAND_MODELS = new Set(["plus", "tshape", "ushape", "lowledge", "lbracket", "staircase"]);
const POSES = [[0, 0, 0], [40, 0, 0], [0, 35, 20], [90, 0, 0], [-45, 30, 15]];
const OPTS = {
  mid: { coverage: 0.5 },
  one: { coverage: 0.1 },
  dense: { coverage: 0.9 },
  norows: { coverage: 0.5, longWalls: false },
  plate: { coverage: 0.5, plateOnly: true },
  apart: { coverage: 0.6, separateWalls: true },
  notines: { coverage: 0.5, tines: false, layerHeight: 0.3 },
  isl: { coverage: 0.5, withIslands: true },
  reg: { coverage: 0.5, withRegions: true },
};
const t0 = performance.now();
for (const name of MODELS) {
  const topo = topoOf(name);
  for (const deg of POSES) {
    const rot = sitePose(deg);
    const res = analyze(topo, 45, rot);
    const result = automaticTargets(topo, res, rot);
    const built = finsMod.buildFins(topo, res, rot, { mode: "auto", bedPad: true, tines: true, layerHeight: 0.2, tallRibs: false });
    // bareArea
    for (const coverage of [undefined, 0.2, 0.9]) {
      const fins = [...built.fins, { kind: "sway", line: [[0, 0, 5], [1, 0, 5]] }];
      const o = { coverage };
      cases.bare.push({ id: `${name}|r${deg.join(",")}|c${coverage}`, args: [topoRef(name), new Ref({ $analysis: { model: name, threshold: 45, rot: encode(Array.from(rot)) } }), rot, o, fins.map((f) => ({ kind: f.kind, line: f.line })), cfg()], out: fillMod.bareArea(topo, result, rot, o, { fins }) });
    }
    cases.bare.push({ id: `${name}|r${deg.join(",")}|nofins`, args: [topoRef(name), new Ref({ $analysis: { model: name, threshold: 45, rot: encode(Array.from(rot)) } }), rot, { coverage: 0.5 }, [], cfg()], out: fillMod.bareArea(topo, result, rot, { coverage: 0.5 }, { fins: [] }) });
    for (const [on, spec] of Object.entries(OPTS)) {
      if (on !== "mid" && !["cube", "lbracket", "tube", "portal", "staircase", "fixtures/bowl"].includes(name) && !(ISLAND_MODELS.has(name) && ["isl", "one", "dense"].includes(on))) continue;
      const opts = { coverage: spec.coverage, tines: spec.tines ?? true, layerHeight: spec.layerHeight, longWalls: spec.longWalls, plateOnly: spec.plateOnly, separateWalls: spec.separateWalls };
      opts.islands = spec.withIslands ? findIslands(topo, result, rot) : [];
      if (spec.withRegions) opts.fillRegions = new Set(res.regions.slice(0, 2).map((_, i) => i));
      const args = [topoRef(name), new Ref({ $analysis: { model: name, threshold: 45, rot: encode(Array.from(rot)) } }), rot, slimOpts(opts), slimBuilt(built), cfg()];
      const f = fillMod.fillCoverage(topo, result, rot, opts, built);
      if (f.props.length) withWalls++;
      if (f.stats.rows) withRows++;
      if (f.unserved.islands !== undefined && (opts.islands ?? []).length) withIslands++;
      if (f.stats.ribs) withRibs++;
      if (f.stats.ties) withTies++;
      if (f.stats.capped) capped++;
      cases.fill.push({ id: `${name}|r${deg.join(",")}|${on}`, args, out: fillOut(f) });
    }
  }
}
console.log("fill: direct done in", Math.round((performance.now() - t0) / 1000), "s;", "walls", withWalls, "rows", withRows, "islands", withIslands, "ribs", withRibs, "ties", withTies, "capped", capped);
writeCases("fill", "fillReach", cases.reach);
writeCases("fill", "bareArea", cases.bare);
writeCases("fill", "fillCoverage", cases.fill);

// --- the golden scenes (Full builds) --------------------------------------------------------------------
const holder = {};
const cap = await captureModules(
  [
    { rel: "overhangs.js", fns: ["buildTopology", "analyze"] },
    { rel: "fins/fill.js", fns: ["fillCoverage", "bareArea"] },
  ],
  {
    cap: 1e9, refs: true, quiet: ["buildTopology", "analyze"],
    copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"],
    args: {
      buildTopology: ([g]) => [g.getAttribute("position").array],
      analyze: ([, th, rot], sink) => [sink.model, th, rot],
      // (tests/_scene.js analyzes every scene at 45 degrees)
      // the result is buildFins' augmented copy: rebuilt from the plain analysis in the replay
      fillCoverage: ([topo, , rot, opts, built], sink) => [topo, new Ref({ $analysis: { model: sink.model, threshold: 45, rot: encode(Array.from(rot)) } }), rot, slimOpts(opts), slimBuilt(built)],
      bareArea: ([topo, , rot, opts, built], sink) => [topo, new Ref({ $analysis: { model: sink.model, threshold: 45, rot: encode(Array.from(rot)) } }), rot, { coverage: opts.coverage }, (built.fins ?? []).map((f) => ({ kind: f.kind, line: f.line }))],
    },
    config: { fillCoverage: Object.keys(cfgNames), bareArea: Object.keys(cfgNames) },
    configs: new Proxy({}, { get: (_, k) => holder.thunks?.[k] }),
  },
);
try {
  const mods = {
    PROP: (await import(`${cap.rootUrl}web/prop/config.js`)).PROP,
    FILL: (await import(`${cap.rootUrl}web/fins/fill.js`)).FILL,
    ROWS: (await import(`${cap.rootUrl}web/fins/rows.js`)).ROWS,
    BRACE: (await import(`${cap.rootUrl}web/prop/brace.js`)).BRACE,
    TIE: (await import(`${cap.rootUrl}web/prop/ties.js`)).TIE,
    FIELD: (await import(`${cap.rootUrl}web/fins/field.js`)).FIELD,
  };
  holder.thunks = Object.fromEntries(Object.entries(mods).map(([k, v]) => [k, () => v]));
  const { run: runScene } = await import(`${cap.rootUrl}tests/_scene.js`);
  for (const scene of goldenScenes()) {
    setScene(cap.sink, scene.name);
    cap.sink.model = scene.model;
    runScene(scene);
  }
  const fills = cap.sink.calls.fillCoverage.entries().map((e) => {
    const r = e.out;
    const flat = [];
    for (const v of r.triangles) flat.push(dec(v[0]), dec(v[1]), dec(v[2]), v.length > 3 && dec(v[3]) === 1 ? 1 : 0);
    return {
      id: `${e.scene}#${e.n}`, args: e.args,
      out: { tris: encode(Float64Array.from(flat)), props: r.props, tines: r.tines, unserved: r.unserved, stats: sortKeys(r.stats) },
    };
  });
  const bares = cap.sink.calls.bareArea.entries().map((e) => ({ id: `${e.scene}#${e.n}`, args: e.args, out: e.out }));
  // (the golden scenes build Auto and Prop only: Full never runs, so there is nothing to keep)
  if (fills.length) writeCases("fill", "fillCoverage.captured", fills, { encoded: true });
  writeCases("fill", "bareArea.captured", bares, { encoded: true });
  console.log("fill: direct", cases.fill.length, "bare", cases.bare.length, "captured fills", fills.length, "bares", bares.length,
    "captured walls", fills.filter((c) => c.out.props.length).length, "rows", fills.filter((c) => c.out.stats.rows).length);
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
