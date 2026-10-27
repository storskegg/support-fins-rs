// Records parity/corpus/tall/*.jsonl from web/fins/tall.js: `planGrid` (direct), and `braceTallWalls` on real builds
// (`direct-*`: buildFins with the ribs off, then braceTallWalls with BRACE.tallH lowered so walls qualify) and as
// the 54 golden scenes' builds call it (`*.captured`, named by scene).
//   planGrid         args: [adds, queries, {PROP}]    adds: [{vs: F64 (x, y, z per vertex), tag: number | null, keepBelow: number | null}]
//                    (keepBelow: a `keep` that accepts triangles whose lowest z <= it, as braceTallWalls' `low`; null keeps all)
//                    queries: [{vs: F64, own: number | undefined}]       out: [boolean]  (hits, in order, after all adds)
//   braceTallWalls   args: [$topo, {offset}, rot, built, {PROP, BRACE}]
//                    built: {fins: [{kind, braces?, height, line, triRanges, fill?}], triangles: F64 (4 per vertex), padTriangles: F64}
//                    out: {tris: F64 (4 per vertex, after), tallRibs, fins: [{triRanges, ribs?}]}
import { assertPinned, WEB } from "./lib/pin.js";
import { captureModules, setScene, upstreamStatus } from "./lib/capture.js";
import { encode, Ref, writeCases } from "./lib/encode.js";
import { analyze, cfgMods, modelList, sitePose, topoOf, topoRef } from "./lib/direct.js";
import { goldenScenes } from "./lib/golden.js";

assertPinned();
const statusBefore = upstreamStatus();
const { BRACE } = await import(`${WEB}prop/brace.js`);
const PROP = cfgMods.PROP;
const tall = await import(`${WEB}fins/tall.js`);
const finsMod = await import(`${WEB}fins.js`);
const cfg = () => new Ref({ $config: { PROP: encode(PROP), BRACE: encode(BRACE) } });
const dec = (v) => (typeof v === "number" ? v : v.$n === "-0" ? -0 : v.$n === "NaN" ? NaN : v.$n === "Infinity" ? Infinity : -Infinity);
const flat4 = (list) => {
  const a = [];
  for (const v of list ?? []) a.push(v[0], v[1], v[2], v[3] === 1 ? 1 : 0);
  return Float64Array.from(a);
};
const flat3 = (list) => {
  const a = [];
  for (const v of list) a.push(v[0], v[1], v[2]);
  return Float64Array.from(a);
};
/** What braceTallWalls reads of `built` (copied: the call replaces fields). */
function slimBuilt(built) {
  return structuredClone({
    fins: (built.fins ?? []).map((f) => ({ kind: f.kind, braces: f.braces, height: f.height, line: f.line, triRanges: f.triRanges, fill: f.fill })),
    triangles: flat4(built.triangles), padTriangles: flat4(built.padTriangles),
  });
}
const outOf = (built) => ({
  tris: flat4(built.triangles), tallRibs: built.tallRibs,
  fins: (built.fins ?? []).map((f) => ({ triRanges: f.triRanges, ...(f.ribs !== undefined ? { ribs: f.ribs } : {}) })),
});

const cases = { grid: [], direct: [] };

// --- planGrid ----------------------------------------------------------------------------------------------
{
  let seed = 12345;
  const rnd = () => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff);
  const tri = (cx, cy, size, z) => {
    const v = [];
    for (let k = 0; k < 3; k++) v.push(cx + (rnd() - 0.5) * size, cy + (rnd() - 0.5) * size, z + (rnd() - 0.5) * 8);
    return v;
  };
  for (let c = 0; c < 60; c++) {
    const adds = [], queries = [];
    const nAdd = 1 + Math.floor(rnd() * 6);
    for (let a = 0; a < nAdd; a++) {
      const n = 1 + Math.floor(rnd() * 5), vs = [];
      const size = [1, 3, 10, 40][Math.floor(rnd() * 4)];
      const cx = (rnd() - 0.5) * 60, cy = (rnd() - 0.5) * 60, z = rnd() * 30;
      for (let i = 0; i < n; i++) vs.push(...tri(cx, cy, size, z));
      adds.push({ vs: Float64Array.from(vs), tag: a % 3 === 0 ? null : a % 3, keepBelow: a % 4 === 0 ? 12 : null });
    }
    for (let q = 0; q < 8; q++) {
      const n = 1 + Math.floor(rnd() * 3), vs = [];
      const cx = (rnd() - 0.5) * 60, cy = (rnd() - 0.5) * 60, z = rnd() * 30;
      for (let i = 0; i < n; i++) vs.push(...tri(cx, cy, [0.5, 2, 8][Math.floor(rnd() * 3)], z));
      queries.push({ vs: Float64Array.from(vs), own: q % 3 === 0 ? undefined : q % 3 });
    }
    const g = tall.planGrid();
    for (const a of adds) {
      const v = [];
      for (let i = 0; i < a.vs.length; i += 3) v.push([a.vs[i], a.vs[i + 1], a.vs[i + 2]]);
      g.add(v, a.tag, a.keepBelow === null ? undefined : (p, q, r) => Math.min(p[2], q[2], r[2]) <= a.keepBelow);
    }
    const out = queries.map((q) => {
      const v = [];
      for (let i = 0; i < q.vs.length; i += 3) v.push([q.vs[i], q.vs[i + 1], q.vs[i + 2]]);
      return g.hits(v, q.own);
    });
    cases.grid.push({ id: `grid${c}`, args: [adds, queries, cfg()], out });
  }
}

// --- braceTallWalls on real builds -------------------------------------------------------------------------
let ribbed = 0, braced = 0;
const MODELS = modelList().map(([n]) => n);
const POSES = [[0, 0, 0], [40, 0, 0], [90, 0, 0], [0, 35, 20]];
for (const name of MODELS) {
  const topo = topoOf(name);
  for (const deg of POSES) {
    const rot = sitePose(deg);
    const res = analyze(topo, 45, rot);
    for (const [mode, tallH] of [["auto", 100], ["auto", 30], ["full", 60]]) {
      if (mode === "full" && !["lbracket", "tube", "cone"].includes(name)) continue;
      const was = BRACE.tallH;
      BRACE.tallH = tallH;
      try {
        const built = finsMod.buildFins(topo, res, rot, { mode, bedPad: true, tines: true, layerHeight: 0.2, tallRibs: false });
        if (!(built.fins ?? []).some((f) => f.height > tallH)) continue;
        if (built.triangles.length > 15000) continue; // a corpus case is megabytes of vertices: keep the smaller builds
        const args = [topoRef(name), { offset: res.offset }, rot, slimBuilt(built), cfg()];
        tall.braceTallWalls(topo, res, rot, built);
        braced++;
        if (built.tallRibs > 0) ribbed++;
        cases.direct.push({ id: `${name}|r${deg.join(",")}|${mode}|h${tallH}`, args, out: outOf(built) });
      } finally {
        BRACE.tallH = was;
      }
    }
  }
}
writeCases("tall", "planGrid", cases.grid);
writeCases("tall", "braceTallWalls", cases.direct);

// --- the golden scenes ---------------------------------------------------------------------------------------
const holder = {};
const cap = await captureModules(
  [
    { rel: "overhangs.js", fns: ["buildTopology", "analyze"] },
    { rel: "fins/tall.js", fns: ["braceTallWalls"] },
  ],
  {
    cap: 1e9, refs: true, quiet: ["buildTopology", "analyze"],
    copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"],
    args: {
      buildTopology: ([g]) => [g.getAttribute("position").array],
      analyze: ([, th, rot], sink) => [sink.model, th, rot],
      braceTallWalls: ([topo, result, rot, built]) => [topo, { offset: result.offset }, rot, slimBuilt(built)],
    },
    config: { braceTallWalls: ["PROP", "BRACE"] },
    configs: new Proxy({}, { get: (_, k) => holder.thunks?.[k] }),
  },
);
try {
  const propMod = await import(`${cap.rootUrl}web/prop/config.js`);
  const braceMod = await import(`${cap.rootUrl}web/prop/brace.js`);
  holder.thunks = { PROP: () => propMod.PROP, BRACE: () => braceMod.BRACE };
  const { run: runScene } = await import(`${cap.rootUrl}tests/_scene.js`);
  // braceTallWalls mutates `built`; the capture's out is encode(built) after the call
  for (const scene of goldenScenes()) {
    setScene(cap.sink, scene.name);
    cap.sink.model = scene.model;
    runScene(scene);
  }
  // the scenes' walls are rarely tall: keep every call that has a tall wall to look at, and a few of the rest
  const entries = cap.sink.calls.braceTallWalls.entries();
  const hasTall = (e) => e.args[3].fins.some((f) => f.height > BRACE.tallH && f.line.length >= 2);
  const chosen = entries.filter((e, i) => hasTall(e) || i % 9 === 0).filter((e) => e.args[3].triangles.$f64.length < 600000);
  const out = chosen.map((e) => {
    const r = e.out;
    const flat = [];
    for (const v of r.triangles) flat.push(dec(v[0]), dec(v[1]), dec(v[2]), v.length > 3 && dec(v[3]) === 1 ? 1 : 0);
    return {
      id: `${e.scene}#${e.n}`, args: e.args,
      out: {
        tris: encode(Float64Array.from(flat)), tallRibs: r.tallRibs,
        fins: r.fins.map((f) => ({ triRanges: f.triRanges, ...(f.ribs !== undefined ? { ribs: f.ribs } : {}) })),
      },
    };
  });
  writeCases("tall", "braceTallWalls.captured", out, { encoded: true });
  console.log("tall: grid", cases.grid.length, "direct", cases.direct.length, "ribbed", ribbed, "captured", out.length,
    "captured ribbed", out.filter((c) => c.out.tallRibs > 0).length);
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
