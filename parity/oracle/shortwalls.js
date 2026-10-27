// Records parity/corpus/shortwalls/lastResortWalls.jsonl from web/fins/shortwalls.js: direct calls on the stress models
// (`direct-*`) and as the 54 golden scenes' Auto builds call it (`*.captured`).
//   lastResortWalls  args: [$topo, $analysis, rot, opts, servedRegions ([i...]), wedgeTris (F64 x, y, z), existing (F64 x, y, z), state, {PROP}]
//     $analysis  the plain analysis; `result` is buildFins' augmented copy, which the replay rebuilds with automaticTargets
//     opts       the keys the prop pipeline reads (step, coverage, tines, tineDensity, layerHeight, raster, sway {on}, rimWalls,
//                downhill, curvedWalls, plateOnly)
//     state      prop/crest.js's module state {part: $seated | null, layerHeight} at the call
//     out        {tris (F64, 4 per vertex), props: [{region, id, tines, span, height, line, triRanges}] (each is `short: true`), served, tines}
import { assertPinned, WEB } from "./lib/pin.js";
import { captureModules, setScene, upstreamStatus } from "./lib/capture.js";
import { encode, Ref, writeCases } from "./lib/encode.js";
import { REGION_MODELS, analyze, cfgMods, sitePose, topoOf, topoRef } from "./lib/direct.js";
import { goldenScenes } from "./lib/golden.js";

assertPinned();
const statusBefore = upstreamStatus();
const PROP = cfgMods.PROP;
const short = await import(`${WEB}fins/shortwalls.js`);
const { automaticTargets } = await import(`${WEB}fins/coverage.js`);
const { buildProps } = await import(`${WEB}prop.js`);
const { crestPart } = await import(`${WEB}prop/crest.js`);
const cfg = () => new Ref({ $config: { PROP: encode(PROP) } });
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
  step: o.step, coverage: o.coverage, tines: o.tines, tineDensity: o.tineDensity, layerHeight: o.layerHeight, raster: o.raster,
  sway: o.sway ? { on: o.sway.on } : undefined, rimWalls: o.rimWalls, downhill: o.downhill, curvedWalls: o.curvedWalls, plateOnly: o.plateOnly,
});
const slimProp = (q) => ({ region: q.region, id: q.id, tines: q.tines, span: q.span, height: q.height, line: q.line, triRanges: q.triRanges });
const outOf = (r) => ({ tris: flat4(r.triangles), props: r.props.map(slimProp), served: r.served, tines: r.tines });
const anaRef = (name, rot) => new Ref({ $analysis: { model: name, threshold: 45, rot: encode(Array.from(rot)) } });

const direct = [];
let withWalls = 0;
for (const name of REGION_MODELS) {
  const topo = topoOf(name);
  for (const deg of [[0, 0, 0], [40, 0, 0], [0, 35, 20], [90, 0, 0], [-45, 30, 15]]) {
    const rot = sitePose(deg);
    const res = analyze(topo, 45, rot);
    const result = automaticTargets(topo, res, rot);
    for (const [on, opts0] of [["tines", { tines: true, coverage: 0.5 }], ["plain", { coverage: 0.2 }]]) {
      crestPart(null);
      const base = buildProps(topo, result, rot, { ...opts0, raster: false });
      for (const [vn, served, existing] of [["nothing", [], []], ["base", base.servedRegions ?? [], base.triangles]]) {
        crestPart(null);
        const r = short.lastResortWalls(topo, result, rot, opts0, served, [], existing);
        if (r.props.length) withWalls++;
        direct.push({
          id: `${name}|r${deg.join(",")}|${on}|${vn}`,
          args: [topoRef(name), anaRef(name, rot), rot, slimOpts(opts0), served, flat3([]), flat3(existing), { part: null, layerHeight: undefined }, cfg()],
          out: outOf(r),
        });
      }
    }
  }
}
writeCases("shortwalls", "lastResortWalls", direct);

// --- the golden scenes ------------------------------------------------------------------------------------
const holder = {};
const cap = await captureModules(
  [
    { rel: "overhangs.js", fns: ["buildTopology", "analyze"] },
    { rel: "fins/seating.js", fns: ["seatedPartTris"] },
    { rel: "prop/crest.js", fns: ["crestPart"] },
    { rel: "fins/shortwalls.js", fns: ["lastResortWalls"] },
  ],
  {
    cap: 1e9, refs: true, quiet: ["buildTopology", "analyze", "seatedPartTris"],
    copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"],
    args: {
      buildTopology: ([g]) => [g.getAttribute("position").array],
      analyze: ([, th, rot], sink) => [sink.model, th, rot],
      lastResortWalls: ([topo, , rot, opts, served, wedgeTris, existing], sink) => [topo, anaRef(sink.model, rot), rot, slimOpts(opts), [...served], flat3(wedgeTris), flat3(existing)],
    },
    config: { lastResortWalls: ["PROP"] },
    configs: new Proxy({}, { get: (_, k) => holder.thunks?.[k] }),
  },
);
try {
  const propMod = await import(`${cap.rootUrl}web/prop/config.js`);
  holder.thunks = { PROP: () => propMod.PROP };
  const { run: runScene } = await import(`${cap.rootUrl}tests/_scene.js`);
  for (const scene of goldenScenes()) {
    setScene(cap.sink, scene.name);
    cap.sink.model = scene.model;
    runScene(scene);
  }
  const cpLog = new Map(); // scene -> [crestPart call args]
  for (const e of cap.sink.calls.crestPart.entries()) {
    if (!cpLog.has(e.scene)) cpLog.set(e.scene, []);
    cpLog.get(e.scene)[e.n] = e.args;
  }
  const stateOf = (e) => {
    const a = e.cp > 0 ? cpLog.get(e.scene)?.[e.cp - 1] : null;
    return a ? { part: a[0], layerHeight: a[1] ?? { $u: 1 } } : { part: null, layerHeight: { $u: 1 } };
  };
  const out = cap.sink.calls.lastResortWalls.entries().map((e) => {
    const r = e.out;
    const flat = [];
    for (const v of r.triangles) flat.push(dec(v[0]), dec(v[1]), dec(v[2]), v.length > 3 && dec(v[3]) === 1 ? 1 : 0);
    return {
      id: `${e.scene}#${e.n}`,
      args: [...e.args.slice(0, 7), stateOf(e), e.args.at(-1)],
      out: {
        tris: encode(Float64Array.from(flat)),
        props: r.props.map((q) => ({ region: q.region, id: q.id, tines: q.tines, span: q.span, height: q.height, line: q.line, triRanges: q.triRanges })),
        served: r.served, tines: r.tines,
      },
    };
  });
  writeCases("shortwalls", "lastResortWalls.captured", out, { encoded: true });
  console.log("shortwalls: direct", direct.length, "with walls", withWalls, "captured", out.length, "captured with walls", out.filter((c) => c.out.props.length).length);
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
