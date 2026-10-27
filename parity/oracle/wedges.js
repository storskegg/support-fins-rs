// Records parity/corpus/wedges/*.jsonl from web/fins/wedges.js: calls made directly (`direct-*` scenes) and as the 54
// golden scenes' builds make them (`build-*`). Every case ends with {"$config": {...}} (the objects named below).
//   perpColumns        args: [patch, lo, hi, pitch, {PERP}]                                    out: [u...]
//   buildPerpFins      args: [patch, $topo, rot, offset, opts, state, {PERP, PROP, CUT}]
//                      out: {tris, tines, count, wedges}: `tris` a Float64Array of 4 numbers per vertex of `triangles`
//                      (x, y, z, 1 where v[3] === 1 else 0); `wedges` [{triRange, line, height, span}]
//                      `state` is the crest's module state {part, layerHeight} at the call
//   propServesPatch    args: [patch, props, {PROP}]    props: [{line, squat?}] (the only keys it reads)   out: boolean
//   bareAfterWedges    args: [$topo, rot, {offset, regions: [{faces}]}, servedRegions ([i...]), wedgeTris (Float64Array of x, y, z), {PROP}]
//                      out: [region index...]   (unservedAfterWedges is its length; recorded the same way)
//   unservedAfterWedges  same args, out: number
//   gripPatches        (direct only) args: [$topo, $analysis, rot]   out: {patches, faceMap: [[face, index into patches]...]}
//                      (JS maps each face to the patch object; the index says which)
//   wedgeVeto          (direct only) args: [patches, $topo, rot, offset, wedgeOpts, propSets, queries, {PERP, PROP, CUT}]
//                      propSets: [[{line, squat?}...]...]; queries: [[set index, q]...] answered in order by ONE closure
//                      out: [boolean...]
import { assertPinned, UPSTREAM_ROOT } from "./lib/pin.js";
import { captureModules, setScene, upstreamStatus } from "./lib/capture.js";
import { encode, Ref, writeCases } from "./lib/encode.js";
import { modelList } from "./lib/direct.js";
import { goldenScenes } from "./lib/golden.js";

assertPinned();
const statusBefore = upstreamStatus();

const holder = {};
const cap = await captureModules(
  [
    { rel: "overhangs.js", fns: ["buildTopology", "analyze"] },
    { rel: "fins/seating.js", fns: ["seatedPartTris"] },
    { rel: "prop/crest.js", fns: ["crestPart"] },
    { rel: "fins/wedges.js", fns: ["perpColumns", "buildPerpFins", "propServesPatch", "bareAfterWedges", "unservedAfterWedges"] },
  ],
  {
    cap: 1e9,
    refs: true,
    quiet: ["buildTopology", "analyze", "seatedPartTris"],
    copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"],
    args: {
      buildTopology: ([g]) => [g.getAttribute("position").array],
      analyze: ([, th, rot], sink) => [sink.model, th, rot],
      propServesPatch: ([p, props]) => [p, (props ?? []).map((q) => (q.squat ? { line: q.line, squat: q.squat } : { line: q.line }))],
      bareAfterWedges: ([topo, rot, result, served, tris]) => [topo, rot, slim(result), [...served], flat3(tris)],
      unservedAfterWedges: ([topo, rot, result, served, tris]) => [topo, rot, slim(result), [...served], flat3(tris)],
    },
    config: {
      perpColumns: ["PERP"],
      buildPerpFins: ["PERP", "PROP", "CUT"],
      propServesPatch: ["PROP"],
      bareAfterWedges: ["PROP"],
      unservedAfterWedges: ["PROP"],
    },
    configs: new Proxy({}, { get: (_, k) => holder.thunks?.[k] }),
  },
);
// the only parts of the analyze result these two read (buildFins passes a spread copy, which is no registered reference)
function slim(result) {
  return { offset: result.offset, regions: result.regions.map((g) => ({ faces: g.faces })) };
}
function flat3(tris) {
  const a = new Float64Array(tris.length * 3);
  tris.forEach((v, i) => { a[i * 3] = v[0]; a[i * 3 + 1] = v[1]; a[i * 3 + 2] = v[2]; });
  return a;
}
const dec = (v) => (typeof v === "number" ? v : v.$n === "-0" ? -0 : v.$n === "NaN" ? NaN : v.$n === "Infinity" ? Infinity : -Infinity);

try {
  const wedgeMod = await import(`${cap.rootUrl}web/fins/wedges.js`);
  const propMod = await import(`${cap.rootUrl}web/prop/config.js`);
  const cutMod = await import(`${cap.rootUrl}web/cutout.js`);
  const { PERP } = wedgeMod, { PROP } = propMod, { CUT } = cutMod;
  holder.thunks = { PERP: () => PERP, PROP: () => PROP, CUT: () => CUT };
  const { buildTopology, analyze } = cap.modules["overhangs.js"];
  const { seatedPartTris } = cap.modules["fins/seating.js"];
  const { crestPart } = cap.modules["prop/crest.js"];
  const W = cap.modules["fins/wedges.js"];
  const { findWallPatches } = await import(`${cap.rootUrl}web/planes.js`);
  const { readSTL } = await import(`${cap.rootUrl}web/stl.js`);
  const { sitePose, run: runScene } = await import(`${cap.rootUrl}tests/_scene.js`);
  const realWedgeVeto = wedgeMod.wedgeVeto;
  const realGrip = wedgeMod.gripPatches;

  const LIMIT = 30000; // vertices of one buildPerpFins call kept in the corpus
  const dropBig = (n) => {
    if (n > LIMIT) { const smp = cap.sink.calls.buildPerpFins; smp.first.pop(); smp.total--; }
  };
  const withCfg = (over, fn) => {
    const mods = { PERP, PROP, CUT };
    const saved = {};
    for (const [name, fields] of Object.entries(over)) {
      saved[name] = {};
      for (const [k, v] of Object.entries(fields)) { saved[name][k] = mods[name][k]; mods[name][k] = v; }
    }
    try { return fn(); } finally {
      for (const [name, fields] of Object.entries(saved)) for (const [k, v] of Object.entries(fields)) mods[name][k] = v;
    }
  };

  const extra = []; // cases of the functions that are not captured: [file, case]
  const topoRef = (name) => new Ref({ $topo: name });
  const analysisRef = (name, rot) => new Ref({ $analysis: { model: name, threshold: encode(45), rot: encode(Array.from(rot)) } });
  const snap = (...names) => ({ $config: Object.fromEntries(names.map((n) => [n, encode({ PERP, PROP, CUT }[n])])) });

  // --- direct: model x pose --------------------------------------------------------------------------
  const MODELS = ["cube", "wedge", "lbracket", "ramp", "arch", "cone", "tube", "portal", "lowledge", "staircase", "torus", "fixtures/bowl"];
  const POSES = [[40, 0, 0], [0, 35, 20], [45, 0, 0], [90, 0, 0], [-45, 30, 15], [60, 0, 90]];
  const CONFIGS = [
    ["plain", {}, false],
    ["thick", { PERP: { th: 2.4 } }, false],
    ["crest", { PROP: { iface: "all" } }, true],
    ["cut", { CUT: { pattern: "lattice" } }, false],
    ["round", { PERP: { roundFeet: true, footH: 0.8 } }, false],
  ];
  const OPTS = [["", {}], ["nt", { tines: false }], ["p10", { pitch: 10, tineDensity: 0.4, layerHeight: 0.4 }]];
  const byName = new Map(modelList());
  let nWedge = 0, nVeto = 0;
  for (const name of MODELS) {
    const path = byName.get(name);
    if (!path) throw new Error(`no model ${name}`);
    const pos = readSTL(Deno.readFileSync(path));
    for (const deg of POSES) {
      cap.sink.model = name;
      const topo = buildTopology({ getAttribute: () => ({ array: pos }) });
      const rot = sitePose(deg);
      const res = analyze(topo, 45, rot);
      const offset = res.offset;
      const soup = seatedPartTris(topo, rot, offset);
      const patches = findWallPatches(topo, rot, offset).filter((p) => p.n.z < -0.05);
      const tag = `${name}-r${deg.join(",")}`;
      // grip patches (direct only)
      {
        const g = realGrip(topo, res, rot);
        extra.push(["gripPatches", {
          id: tag, args: [topoRef(name), analysisRef(name, rot), Array.from(rot)],
          out: { patches: g.patches, faceMap: Array.from(g.faceMap, ([f, p]) => [f, g.patches.indexOf(p)]) },
        }]);
      }
      const chosen = patches.filter((p) => p.area >= 30).slice(0, 4);
      // perpColumns over each patch, a few pitches
      for (const [pi, p] of chosen.entries()) {
        for (const pitch of [24, 10, 3]) {
          setScene(cap.sink, `direct-cols-${tag}-p${pi}-${pitch}`);
          W.perpColumns(p, p.u0 + PERP.inset, p.u1 - PERP.inset, pitch);
        }
        setScene(cap.sink, `direct-cols-${tag}-p${pi}-wide`);
        W.perpColumns(p, p.u0, p.u1, 24);
      }
      // wedges
      for (const [cn, over, crestOn] of CONFIGS) {
        // the heavier configs on the largest patches only
        const use = cn === "plain" ? chosen : chosen.slice(0, 2);
        withCfg(over, () => {
          for (const [pi, p] of use.entries()) {
            for (const [on, opts] of OPTS) {
              if (cn !== "plain" && on !== "") continue;
              setScene(cap.sink, `direct-fins-${tag}-p${pi}-${cn}-${on}`);
              if (crestOn) crestPart(soup, 0.2); else crestPart(null);
              const r = W.buildPerpFins(p, topo, rot, offset, opts);
              dropBig(r.triangles.length);
              nWedge++;
            }
          }
        });
      }
      crestPart(null);
      // the wedges of every chosen patch, then what is left unserved
      setScene(cap.sink, `direct-after-${tag}`);
      const wedgeTris = [];
      for (const [wi, p] of patches.filter((p) => p.area >= PERP.minArea && p.u1 - p.u0 >= PERP.minWidth).entries()) {
        setScene(cap.sink, `direct-after-${tag}-w${wi}`);
        const w = W.buildPerpFins(p, topo, rot, offset, {});
        dropBig(w.triangles.length);
        for (const v of w.triangles) wedgeTris.push(v);
      }
      setScene(cap.sink, `direct-after-${tag}`);
      for (const served of [[], res.regions.length ? [0] : []]) {
        W.bareAfterWedges(topo, rot, res, served, wedgeTris);
        W.unservedAfterWedges(topo, rot, res, new Set(served), wedgeTris);
      }
      W.bareAfterWedges(topo, rot, res, [], []);
      // propServesPatch + the veto
      const mid = (p) => {
        const u = (p.u0 + p.u1) / 2, t = (p.t0 + p.t1) / 2;
        return [p.n.x * p.d + p.u.x * u + p.t.x * t, p.n.y * p.d + p.u.y * u + p.t.y * t, p.n.z * p.d + p.t.z * t];
      };
      const wedgeable = patches.filter((p) => p.area >= PERP.minArea && p.u1 - p.u0 >= PERP.minWidth);
      const sets = [];
      const near = (p, z) => { const m = mid(p); return { line: [[m[0], m[1], z], [m[0] + 1, m[1] + 1, z]] }; };
      sets.push([]);
      for (const [si, p] of patches.slice(0, 3).entries()) {
        setScene(cap.sink, `direct-serves-${tag}-${si}`);
        const mine = [near(p, 8)], far = [{ line: [[m3(p)[0] + 300, 0, 9]] }], low = [near(p, 0.5)], squat = [{ ...near(p, 0.5), squat: true }];
        for (const props of [[], mine, far, low, squat, [...far, ...mine]]) {
          for (const q of patches.slice(0, 3)) W.propServesPatch(q, props);
        }
        if (wedgeable.length) sets.push(mine, far, low, squat);
      }
      function m3(p) { return mid(p); }
      if (wedgeable.length) {
        const wedgeOpts = { tines: true, pitch: 24, tineDensity: 0.5, layerHeight: 0.2 };
        const closure = realWedgeVeto(wedgeable, topo, rot, offset, wedgeOpts);
        const queries = [], outs = [];
        const uniq = [];
        for (const s of sets) if (!uniq.some((u) => JSON.stringify(u) === JSON.stringify(s))) uniq.push(s);
        for (const [si, s] of uniq.entries()) {
          for (const q of [...wedgeable.slice(0, 3).map((p) => near(p, 8)), { line: [[400, 400, 9]] }, { ...near(wedgeable[0], 0.5), squat: true }]) {
            queries.push([si, q]);
            outs.push(closure(q, s));
          }
        }
        // the same sets again (the memo hit), then one fresh set
        for (const [si, q] of queries.slice(0, 4)) outs.push(closure(q, uniq[si])), queries.push([si, q]);
        nVeto++;
        extra.push(["wedgeVeto", { id: tag, args: [wedgeable, topoRef(name), Array.from(rot), offset, wedgeOpts, uniq, queries, snap("PERP", "PROP", "CUT")], out: outs }]);
      }
    }
  }
  // synthetic patches: a clean face and a bored one (tests/perp_hole.test.js), against the cube
  {
    const quad = (u0, u1, t0, t1) => [u0, t0, 0, u1, t0, 0, u1, t1, 0, u0, t0, 0, u1, t1, 0, u0, t1, 0];
    const patch = (tris, uLo, uHi, tTop) => ({ tris: new Float32Array(tris), u0: uLo, u1: uHi, t0: 0, t1: tTop, n: { x: 0, y: 0, z: -1 }, d: 0, u: { x: 1, y: 0 }, t: { x: 0, y: 0, z: 1 } });
    const solid = patch(quad(-30, 30, 0, 40), -30, 30, 40);
    const bored = patch([...quad(-30, -8, 0, 40), ...quad(8, 30, 0, 40), ...quad(-8, 8, 0, 10), ...quad(-8, 8, 30, 40)], -30, 30, 40);
    const two = patch([...quad(-30, -12, 0, 40), ...quad(-4, 4, 0, 40), ...quad(12, 30, 0, 40)], -30, 30, 40);
    const rim = patch([...quad(-30, 30, 0, 14), ...quad(-30, 30, 16, 40)], -30, 30, 40);
    for (const [pn, p] of Object.entries({ solid, bored, two, rim })) {
      for (const [lo, hi, pitch] of [[-28, 28, 24], [-28, 28, 6], [-28, 28, 100], [0, 5, 24], [5, 5, 24], [10, 2, 24], [-30, 30, 2]]) {
        setScene(cap.sink, `direct-synthetic-${pn}-${lo}-${hi}-${pitch}`);
        W.perpColumns(p, lo, hi, pitch);
      }
    }
  }

  // --- the golden scenes' own builds ----------------------------------------------------------------------
  for (const scene of goldenScenes()) {
    setScene(cap.sink, `build-${scene.name}`);
    cap.sink.model = scene.model;
    runScene(scene);
  }

  // --- assemble ---------------------------------------------------------------------------------------------
  const cpLog = new Map(); // scene -> [crestPart call args]
  for (const e of cap.sink.calls.crestPart.entries()) {
    if (!cpLog.has(e.scene)) cpLog.set(e.scene, []);
    cpLog.get(e.scene)[e.n] = e.args;
  }
  const stateOf = (e) => {
    const a = e.cp > 0 ? cpLog.get(e.scene)?.[e.cp - 1] : null;
    return a ? { part: a[0], layerHeight: a[1] ?? { $u: 1 } } : { part: null, layerHeight: { $u: 1 } };
  };
  const idOf = (e) => `${e.scene}#${e.n}`;
  const out = {};
  for (const fn of ["perpColumns", "propServesPatch", "bareAfterWedges", "unservedAfterWedges"]) {
    out[fn] = cap.sink.calls[fn].entries().map((e) => ({ id: idOf(e), args: e.args, out: e.out }));
  }
  out.buildPerpFins = cap.sink.calls.buildPerpFins.entries().map((e) => {
    const r = e.out;
    const flat = [];
    for (const v of r.triangles) flat.push(dec(v[0]), dec(v[1]), dec(v[2]), v.length > 3 && dec(v[3]) === 1 ? 1 : 0);
    return {
      id: idOf(e),
      args: [...e.args.slice(0, 5), stateOf(e), e.args.at(-1)],
      out: { tris: encode(Float64Array.from(flat)), tines: r.tines, count: r.count, wedges: r.wedges },
    };
  });
  for (const [fn, list] of Object.entries(out)) {
    // the golden scenes call propServesPatch very often: thin them
    const thin = fn === "propServesPatch" ? list.filter((c, i) => c.id.startsWith("direct-") || i % 7 === 0) : list;
    writeCases("wedges", fn, thin, { encoded: true });
  }
  for (const fn of ["gripPatches", "wedgeVeto"]) writeCases("wedges", fn, extra.filter(([f]) => f === fn).map(([, c]) => c));
  const bf = out.buildPerpFins;
  console.log("wedges:", Object.fromEntries(Object.entries(out).map(([k, v]) => [k, v.length])),
    "built", bf.filter((c) => c.out.count > 0).length, "direct fins", nWedge, "veto sets", nVeto,
    "grip", extra.filter(([f]) => f === "gripPatches").length);
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
