// Records parity/corpus/sway/*.jsonl from web/sway.js: direct calls on synthetic parts (`*.soup`, the replay builds the
// topology and the analysis itself), direct calls on the stress models ($topo references) and, for buildSwayBraces,
// the golden scene that turns sway on (`*.captured`). Every case ends with {"$config": {SWAY, PROP, BRACE}}.
//   Rib records are the JS objects without their triangles unless the case says otherwise:
//     {ok: true, [tris], tines, height, depth, th, stilt, foot, halfW, levels, cross, arms, [profiled], [triRange]}
//   or {ok: false, reason}. `tris` (when kept) is a Float64Array of 4 numbers per vertex (x, y, z, 1 where v[3] === 1).
//   buildSwayBraces.soup  args: [pos (Float32Array), thresholdDeg, rot, opts, config]
//                         out: {triangles (F64, 4 per vertex), count, tines, skipped, ribs (no tris), reason}
//   buildSwayBraces       args: [$topo, rot, {offset}, opts, config]   (models; same out)
//   buildSwayBraces.captured  the golden scene's call; args: [$topo, rot, {offset}, opts, config]
//   buildSwayRib.soup     args: [pos, thr, rot, patchIndex, uc, opts, config]  (patchIndex: into findWallPatches(topo, rot, offset))
//                         out: a rib record WITH its tris, or {ok: false, reason}. The part's triangles are the seated soup.
//   swayAtFace.soup       args: [pos, thr, rot, faceIndex, point, opts, avoid, config]   avoid: [] | [rib...] | {braces?, walls?}
//                         out: a rib record with tris, or {ok: false, reason}
//   swayClashes.soup      args: [rib, ribs, config]     (slim ribs)       out: boolean
//   swayClashesWall.soup  args: [rib, walls, config]                      out: boolean
//   faceIsUpright         args: [$topo, rot, faceIndex, config]           out: boolean
import { assertPinned, WEB } from "./lib/pin.js";
import { encode, Ref, writeCases } from "./lib/encode.js";
import { analyze, buildTopology, cfgMods, sitePose, topoRef, modelList, topoOf } from "./lib/direct.js";
import { captureModules, setScene, upstreamStatus } from "./lib/capture.js";
import { goldenScenes } from "./lib/golden.js";

assertPinned();
const statusBefore = upstreamStatus();
const sway = await import(`${WEB}sway.js`);
const { SWAY } = sway;
const { findWallPatches } = await import(`${WEB}planes.js`);
const { BRACE } = await import(`${WEB}prop/brace.js`);
const { seatedPartTris } = await import(`${WEB}fins/seating.js`);
const PROP = cfgMods.PROP;
const cfg = () => new Ref({ $config: { SWAY: encode(SWAY), PROP: encode(PROP), BRACE: encode(BRACE) } });
const ID = [1, 0, 0, 0, 1, 0, 0, 0, 1];
const f32 = (a) => Float32Array.from(a);

// --- helpers that shape the recorded values -----------------------------------------------------------------
const dec = (v) => (typeof v === "number" ? v : v.$n === "-0" ? -0 : v.$n === "NaN" ? NaN : v.$n === "Infinity" ? Infinity : -Infinity);
const trisF64 = (list) => {
  const a = [];
  for (const v of list) a.push(v[0], v[1], v[2], v[3] === 1 ? 1 : 0);
  return Float64Array.from(a);
};
/** A rib result as recorded: tris as F64, or dropped. */
function ribRec(r, keepTris) {
  if (!r.ok) return r;
  const o = {};
  for (const [k, v] of Object.entries(r)) {
    if (k === "tris") { if (keepTris) o.tris = trisF64(v); } else o[k] = v;
  }
  return o;
}
/** The fields the clash checks read (a brace already standing, as the page sends them back). */
const slim = (r) => ({ foot: r.foot, halfW: r.halfW, th: r.th, height: r.height, levels: r.levels, arms: r.arms, ...(r.profiled ? { profiled: true } : {}) });
const bracesRec = (s) => ({
  triangles: trisF64(s.triangles), count: s.count, tines: s.tines, skipped: s.skipped,
  ribs: s.ribs.map((r) => ribRec(r, false)), reason: s.reason,
});
const block = (x0, x1, y0, y1, z0, z1) => {
  const v = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
  const q = (a, b, c, d) => [v[a], v[b], v[c], v[a], v[c], v[d]];
  return [...q(0, 3, 2, 1), ...q(4, 5, 6, 7), ...q(0, 1, 5, 4), ...q(2, 3, 7, 6), ...q(1, 2, 6, 5), ...q(0, 4, 7, 3)].flat();
};
const cat = (...parts) => f32(parts.flat());
const topoOfPos = (pos) => buildTopology({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });

// --- synthetic parts ---------------------------------------------------------------------------------
const PARTS = {
  post150: cat(block(-20, 20, -15, 15, 0, 150)),
  post60: cat(block(-20, 20, -15, 15, 0, 60)),
  post20: cat(block(-20, 20, -15, 15, 0, 20)),
  wide: cat(block(-60, 60, -10, 10, 0, 120)),
  channel: cat(block(-40, 40, -30, -20, 0, 150), block(-40, 40, 20, 30, 0, 150)),
  pedestal: cat(block(-8, 8, -8, 8, 0, 70), block(-20, 20, -15, 15, 70, 150)),
  // a ledge sticking out above the face caps the rib below it
  shoulder: cat(block(-20, 20, -15, 15, 0, 120), block(-20, 20, -25, 15, 90, 100)),
  // a base that spreads out under the face: no room for the foot
  flared: cat(block(-20, 20, -15, 15, 0, 120), block(-20, 20, -30, 15, 0, 6)),
  // a stepped face: the profiled rib
  stepped: cat(block(-20, 20, -15, 15, 0, 150), block(-20, 20, -20, -15, 0, 60), block(-20, 20, -17, -15, 60, 100)),
  lbar: cat(block(-30, 30, -10, 10, 0, 140), block(-30, 30, 10, 40, 0, 30)),
  tower: cat(block(-12, 12, -12, 12, 0, 200)),
};
const POSE_OF = { default: ID };
const OPTS = {
  base: { tines: true, layerHeight: 0.2 },
  notines: { tines: false, layerHeight: 0.2 },
  grip80: { tines: true, layerHeight: 0.2, gripFrom: 80 },
  sp4: { tines: true, layerHeight: 0.3, tineSpacing: 4, reach: 0.3, gap: 0.25, tineContactOffset: 0.05 },
  shallow: { tines: true, layerHeight: 0.2, reach: 0.05 },
  deep: { tines: true, layerHeight: 0.2, reach: 0.5, uncappedDepth: true },
  round: { tines: true, layerHeight: 0.2, roundFeet: true },
  cross: { tines: true, layerHeight: 0.2, crossAbove: 100, crossRise: 0.5 },
  cross2: { tines: true, layerHeight: 0.2, crossAbove: 40 },
  iface: { tines: true, layerHeight: 0.2, iface: "all" },
  strict: { tines: true, layerHeight: 0.2, allowStilt: false },
  lenient: { tines: true, layerHeight: 0.2, allowStilt: true },
  walls: { tines: true, layerHeight: 0.2, avoid: { walls: [[[0, -25, 0], [30, -25, 0]], [[10, 40, 0]]] } },
  cap: { tines: true, layerHeight: 0.2, maxHeight: 70 },
  capHigh: { tines: true, layerHeight: 0.2, maxHeight: 1000 },
  infcap: { tines: true, layerHeight: 0.2, maxHeight: Infinity, crossAbove: Infinity },
  nan: { tines: true, layerHeight: NaN, gap: NaN, reach: 7 },
};

const cases = { bsb: [], rib: [], face: [], clash: [], wall: [], model: [], up: [] };
let nOk = 0, nBr = 0, nCross = 0, nProfiled = 0, reasons = {};
const note = (r) => {
  if (r.ok) { nOk++; if (r.cross) nCross++; if (r.profiled) nProfiled++; } else { const k = r.reason.slice(0, 28); reasons[k] = (reasons[k] ?? 0) + 1; }
};
const poses = [["id", ID], ["x20", sitePose([20, 0, 0])], ["z40", sitePose([0, 0, 40])]];
for (const [pname, pos] of Object.entries(PARTS)) {
  const topo = topoOfPos(pos);
  for (const [posename, rot] of poses) {
    if (posename !== "id" && !["post150", "wide", "channel", "stepped"].includes(pname)) continue;
    const res = analyze(topo, 45, rot);
    const tag = `${pname}|${posename}`;
    for (const [on, opts] of Object.entries(OPTS)) {
      if (posename !== "id" && !["base", "cross", "cap"].includes(on)) continue;
      const s = sway.buildSwayBraces(topo, res, rot, opts);
      nBr += s.count;
      cases.bsb.push({ id: `${tag}|${on}`, args: [pos, 45, rot, opts, cfg()], out: bracesRec(s) });
    }
    // ribs: each patch, a few columns
    const patches = findWallPatches(topo, rot, res.offset);
    const partTris = seatedPartTris(topo, rot, res.offset);
    patches.forEach((p, pi) => {
      for (const frac of [0.5, 0.15, 0.85]) {
        const uc = p.u0 + (p.u1 - p.u0) * frac;
        for (const on of ["base", "notines", "sp4", "deep", "round", "cross", "cross2", "iface", "strict", "cap", "grip80", "nan"]) {
          if (frac !== 0.5 && !["base", "cap", "cross2"].includes(on)) continue;
          const r = sway.buildSwayRib(p, uc, partTris, topo, rot, res.offset, OPTS[on]);
          note(r);
          cases.rib.push({ id: `${tag}|p${pi}|u${frac}|${on}`, args: [pos, 45, rot, pi, uc, OPTS[on], cfg()], out: ribRec(r, true) });
        }
      }
    });
    // Draw: every face of the post-like parts, a few points
    if (posename === "id") {
      const faces = [];
      for (let f = 0; f < topo.nFaces; f++) if (f % 3 === 0 || pname === "post150") faces.push(f);
      for (const f of faces.slice(0, 14)) {
        const mid = [0, 1, 2].map((k) => (topo.pos[f * 9 + k] + topo.pos[f * 9 + 3 + k] + topo.pos[f * 9 + 6 + k]) / 3);
        const point = [mid[0] + res.offset.x, mid[1] + res.offset.y, mid[2] + res.offset.z];
        for (const [on, opts, avoidKind] of [["base", OPTS.base, "none"], ["full", { ...OPTS.base, full: true }, "none"], ["low", OPTS.base, "low"], ["strict", OPTS.strict, "none"], ["walls", OPTS.base, "wall"]]) {
          const pt = on === "low" ? [point[0], point[1], 10] : on === "base" ? [point[0], point[1], Math.max(point[2], 60)] : point;
          const avoid = avoidKind === "wall" ? { walls: [[[point[0], point[1], 0], [point[0] + 4, point[1], 0]]] } : [];
          const r = sway.swayAtFace(topo, res, rot, f, pt, opts, avoid);
          note(r);
          cases.face.push({ id: `${tag}|f${f}|${on}`, args: [pos, 45, rot, f, pt, opts, avoid, cfg()], out: ribRec(r, true) });
        }
      }
    }
  }
}
// Draw with braces already standing: the channel, facing walls
{
  const pos = PARTS.channel, topo = topoOfPos(pos), res = analyze(topo, 45, ID);
  const faceFacing = (ny, y) => {
    for (let f = 0; f < topo.nFaces; f++) if (topo.nrm[f * 3 + 1] * ny > 0.9 && Math.abs(topo.pos[f * 9 + 1] - y) < 1e-3) return f;
    return -1;
  };
  const inA = faceFacing(1, -20), inB = faceFacing(-1, 20);
  const first = sway.swayAtFace(topo, res, ID, inA, [0, -20, 150], OPTS.base);
  for (const [tag, face, pt, avoid] of [
    ["across", inB, [0, 20, 150], [first]], ["stagger", inB, [20, 20, 150], [first]],
    ["objform", inB, [0, 20, 150], { braces: [first], walls: [] }], ["wall", inA, [0, -20, 150], { walls: [[first.foot[0], first.foot[1]]] }],
    ["near", inA, [4, -20, 150], [first]], ["farwall", inA, [0, -20, 150], { walls: [[[200, 200, 0], [220, 200, 0]]] }],
  ]) {
    const slimAvoid = Array.isArray(avoid) ? avoid.map(slim) : { ...avoid, ...(avoid.braces ? { braces: avoid.braces.map(slim) } : {}) };
    const r = sway.swayAtFace(topo, res, ID, face, pt, OPTS.base, avoid);
    note(r);
    cases.face.push({ id: `channel|${tag}`, args: [pos, 45, ID, face, pt, OPTS.base, slimAvoid, cfg()], out: ribRec(r, true) });
  }
  // clash checks over every pair of ribs of two builds
  const a = sway.buildSwayBraces(topo, res, ID, OPTS.base).ribs;
  const b = sway.buildSwayBraces(topo, res, ID, { ...OPTS.cross2 }).ribs;
  const all = [...a, ...b];
  for (let i = 0; i < all.length; i++) {
    for (const [tag, list] of [["one", [all[(i + 1) % all.length]]], ["all", all.filter((_, j) => j !== i)], ["none", []]]) {
      cases.clash.push({ id: `channel|${i}|${tag}`, args: [slim(all[i]), list.map(slim), cfg()], out: sway.swayClashes(all[i], list) });
    }
    for (const [tag, walls] of Object.entries({
      empty: [], foot: [[all[i].foot[0], all[i].foot[1]]], point: [[[all[i].foot[0][0], all[i].foot[0][1] + 3, 0]]],
      far: [[[300, 300, 0], [310, 300, 0]]], body: [[all[i].levels[1].a, all[i].levels.at(-1).b]], bad: [[]],
      near: [[[all[i].foot[1][0] + 4.5, all[i].foot[1][1], 0], [all[i].foot[1][0] + 9, all[i].foot[1][1] + 4, 0]]],
    })) {
      cases.wall.push({ id: `channel|${i}|${tag}`, args: [slim(all[i]), walls, cfg()], out: sway.swayClashesWall(all[i], walls) });
    }
  }
  // the pedestal's refused stilt, with the lenient / strict hand brace
}
// models: faceIsUpright over every pose, buildSwayBraces where the model is tall enough
for (const [name] of modelList()) {
  const topo = topoOf(name);
  for (const deg of [[0, 0, 0], [40, 0, 0], [0, 35, 20], [90, 0, 0]]) {
    const rot = sitePose(deg);
    const res = analyze(topo, 45, rot);
    const s = sway.buildSwayBraces(topo, res, rot, OPTS.base);
    if (s.count > 0 || !s.reason.startsWith("the part is only")) {
      nBr += s.count;
      cases.model.push({ id: `${name}|r${deg.join(",")}`, args: [topoRef(name), rot, { offset: res.offset }, OPTS.base, cfg()], out: bracesRec(s) });
    }
    for (let f = 0; f < topo.nFaces; f += Math.max(1, Math.floor(topo.nFaces / 24))) {
      cases.up.push({ id: `${name}|r${deg.join(",")}|f${f}`, args: [topoRef(name), rot, f, cfg()], out: sway.faceIsUpright(topo, rot, f) });
    }
  }
}
for (const [fn, key] of [["buildSwayBraces.soup", "bsb"], ["buildSwayRib.soup", "rib"], ["swayAtFace.soup", "face"], ["swayClashes.soup", "clash"], ["swayClashesWall.soup", "wall"], ["buildSwayBraces", "model"], ["faceIsUpright", "up"]]) {
  writeCases("sway", fn, cases[key]);
}

// --- the golden scene that turns sway on ------------------------------------------------------------------
const holder = {};
const cap = await captureModules(
  [
    { rel: "overhangs.js", fns: ["buildTopology", "analyze"] },
    { rel: "sway.js", fns: ["buildSwayBraces"] },
  ],
  {
    cap: 1e9, refs: true, quiet: ["buildTopology", "analyze"],
    copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"],
    args: {
      buildTopology: ([g]) => [g.getAttribute("position").array],
      analyze: ([, th, rot], sink) => [sink.model, th, rot],
      // only `offset` of the analyze result is read; the result buildFins passes is a spread copy
      buildSwayBraces: ([topo, result, rot, opts]) => [topo, rot, { offset: result.offset }, opts],
    },
    config: { buildSwayBraces: ["SWAY", "PROP", "BRACE"] },
    configs: new Proxy({}, { get: (_, k) => holder.thunks?.[k] }),
  },
);
try {
  const swayMod = await import(`${cap.rootUrl}web/sway.js`);
  const propMod = await import(`${cap.rootUrl}web/prop/config.js`);
  const braceMod = await import(`${cap.rootUrl}web/prop/brace.js`);
  holder.thunks = { SWAY: () => swayMod.SWAY, PROP: () => propMod.PROP, BRACE: () => braceMod.BRACE };
  const { run: runScene } = await import(`${cap.rootUrl}tests/_scene.js`);
  let ran = 0;
  for (const scene of goldenScenes()) {
    if (!scene.set?.sway) continue;
    setScene(cap.sink, scene.name);
    cap.sink.model = scene.model;
    runScene(scene);
    ran++;
  }
  const out = cap.sink.calls.buildSwayBraces.entries().map((e) => {
    const r = e.out;
    const flat = [];
    for (const v of r.triangles) flat.push(dec(v[0]), dec(v[1]), dec(v[2]), v.length > 3 && dec(v[3]) === 1 ? 1 : 0);
    return {
      id: `${e.scene}#${e.n}`,
      args: e.args,
      out: { ...r, triangles: encode(Float64Array.from(flat)), ribs: r.ribs.map(({ tris, ...rest }) => rest) },
    };
  });
  writeCases("sway", "buildSwayBraces.captured", out, { encoded: true });
  console.log("sway: scenes", ran, "captured", out.length);
} finally {
  cap.cleanup();
}
console.log("sway:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])), "ribs ok", nOk, "cross", nCross, "profiled", nProfiled, "braces", nBr, JSON.stringify(reasons));
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
