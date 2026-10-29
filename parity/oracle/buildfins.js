// Records parity/corpus/buildfins/buildFins.jsonl (direct) and buildFins.captured.jsonl (the calls the 54
// golden scenes make) from web/fins.js: THE ENGINE'S ACCEPTANCE (M3).
//
//   buildFins  args: [topo, result, rot, opts, {$config: <every config object>}]
//     topo    a `$topo` reference, or (a synthetic part) the position soup in full
//     result  an `$analysis` reference, or (a synthetic part) the threshold, a number
//     opts    the request as plain data: mode, bedPad, plateOnly, longWalls, separateWalls, tallRibs, curvedWalls,
//             tines, tineDensity, layerHeight, coverage, sway ({on, ...}), tunables ({roundFeet, padH, padGrab,
//             padStyle, padCustom, propGap, cutout, iface, nozzle, wallLines}). The config snapshot is the live module
//             state at the call (applyTunables persists in JS), the Rust build applies the tunables to it.
//     out     a SUMMARY of the built object, in the JS key order:
//             { nTriangles, hash: [h1, h2], tagged, nPad, padHash: [h1, h2], mode, props, fins, volume, skipped,
//               rejected, patchCount, patchStats, tines, servedRegions, unserved, sagRisk, seating, tip, braceCount,
//               propCount, fill, curvedFill, curvedDropped, sway, smallHoleRegions, smallHoleMask, tallRibs, floating,
//               bare, pad, triangles? }
//             `hash` is two 32-bit FNV-style hashes over the little-endian words of every vertex as 4 float64s
//             (x, y, z, tag); the cases flagged `full` also carry `triangles` (Float64Array, 4 per vertex).
//             Keys a build does not have are null.
// Replayed by crates/sf-testkit/tests/parity_buildfins.rs with Tol::Exact.
import { UPSTREAM_ROOT, assertPinned } from "./lib/pin.js";
import { Ref, encode, writeCases } from "./lib/encode.js";
import { goldenScenes } from "./lib/golden.js";
import { upstreamStatus } from "./lib/capture.js";

assertPinned();
const statusBefore = upstreamStatus();

function copyTree(from, to) {
  Deno.mkdirSync(to, { recursive: true });
  for (const e of Deno.readDirSync(from)) {
    const s = `${from}/${e.name}`, d = `${to}/${e.name}`;
    if (e.isDirectory) copyTree(s, d); else if (e.isFile) Deno.copyFileSync(s, d);
  }
}
const dir = Deno.makeTempDirSync({ prefix: "sf-buildfins-" });
copyTree(`${UPSTREAM_ROOT}/web`, `${dir}/web`);
Deno.mkdirSync(`${dir}/tests`, { recursive: true });
for (const f of ["_util.js", "_scene.js", "_circular.js", "_bore.js"]) Deno.copyFileSync(`${UPSTREAM_ROOT}/tests/${f}`, `${dir}/tests/${f}`);
copyTree(`${UPSTREAM_ROOT}/prototype/stress/models`, `${dir}/prototype/stress/models`);
copyTree(`${UPSTREAM_ROOT}/tests/fixtures`, `${dir}/tests/fixtures`);

const FNS = { "overhangs.js": ["buildTopology", "analyze"], "fins/seating.js": ["seatedPartTris"], "fins.js": ["buildFins"] };
for (const [rel, list] of Object.entries(FNS)) {
  const file = `${dir}/web/${rel}`;
  let src = Deno.readTextFileSync(file);
  for (const f of list) {
    const re = new RegExp(`^export function ${f}\\(`, "gm");
    if ((src.match(re) ?? []).length !== 1) throw new Error(`buildfins: expected one 'export function ${f}(' in ${rel}`);
    src = src.replace(re, `function __${f}(`);
  }
  src = `const __rec = (name, fn) => function (...args) { return globalThis.__sfFins.call(name, fn, this, args); };\n${src}\n`
    + list.map((f) => `export const ${f} = __rec('${f}', __${f});`).join("\n") + "\n";
  Deno.writeTextFileSync(file, src);
}

const root = `file://${dir}/`;
const m = {};
for (const [k, p] of Object.entries({
  prop: "prop/config.js", fin: "fins/config.js", pad: "fins/pad.js", perp: "fins/wedges.js", sway: "sway.js", cut: "cutout.js",
  island: "islands.js", trace: "prop/trace.js", tie: "prop/ties.js", brace: "prop/brace.js", resin: "prop/resinfoot.js",
  judge: "prop/judge.js", downhill: "prop/downhill.js", field: "fins/field.js", curved: "prop/curved.js", rim: "prop/rim.js",
  arc: "prop/arc.js", fill: "fins/fill.js", rows: "fins/rows.js", nozzle: "nozzle.js",
})) m[k] = await import(root + "web/" + p);
const snap = () => ({
  PROP: m.prop.PROP, FIN: m.fin.FIN, PAD: m.pad.PAD, PERP: m.perp.PERP, SWAY: m.sway.SWAY, CUT: m.cut.CUT, ISLAND: m.island.ISLAND,
  TRACE: m.trace.TRACE, TIE: m.tie.TIE, BRACE: m.brace.BRACE, RESIN_FOOT: m.resin.RESIN_FOOT, JUDGE: m.judge.JUDGE,
  DOWNHILL: m.downhill.DOWNHILL, FIELD: m.field.FIELD, CURVED: m.curved.CURVED, RIM: m.rim.RIM, ARC: m.arc.ARC, FILL: m.fill.FILL,
  ROWS: m.rows.ROWS, NOZZLE: { nozzle: m.nozzle.nozzle, wallLines: m.nozzle.wallLines },
});

const hashOf = (flat) => {
  const w = new Uint32Array(flat.buffer);
  let h1 = 0x811c9dc5, h2 = 0x9747b28c;
  for (let i = 0; i < w.length; i++) { h1 = Math.imul(h1 ^ w[i], 16777619); h2 = Math.imul(h2 ^ w[i], 0x01000193) ^ (h2 >>> 13); }
  return [h1 >>> 0, h2 >>> 0];
};
const flat4 = (tris) => {
  const flat = new Float64Array(tris.length * 4);
  let tagged = 0;
  for (let i = 0; i < tris.length; i++) {
    const v = tris[i];
    flat[i * 4] = v[0]; flat[i * 4 + 1] = v[1]; flat[i * 4 + 2] = v[2];
    if (v[3] === 1) { flat[i * 4 + 3] = 1; tagged++; }
  }
  return { flat, tagged };
};
const nz = (x) => (x === undefined ? null : x);
const TAGS = ["raster", "circular", "rim", "downhill", "curved", "flush"];
const propOf = (q) => ({
  region: nz(q.region), id: nz(q.id), tines: nz(q.tines), caps: nz(q.caps), grip: nz(q.grip), span: nz(q.span), height: nz(q.height),
  area: nz(q.area), stations: nz(q.stations), trimmed: nz(q.trimmed), volume: nz(q.volume), line: nz(q.line), triRanges: nz(q.triRanges),
  triRange: nz(q.triRange), wallRange: nz(q.wallRange), members: nz(q.members), rimWall: !!q.rimWall, partAttached: !!q.partAttached,
  inBore: nz(q.inBore), squat: !!q.squat, tags: TAGS.map((t) => nz(q[t])),
});
const finOf = (f) => ({
  height: nz(f.height), length: nz(f.length), tines: nz(f.tines), rows: nz(f.rows), stilt: nz(f.stilt), lean: nz(f.lean), bearing: nz(f.bearing),
  site: nz(f.site), id: nz(f.id), kind: nz(f.kind), triRanges: nz(f.triRanges), line: nz(f.line), span: nz(f.span), short: !!f.short,
  fill: !!f.fill, joined: !!f.joined, braces: nz(f.braces), ribs: nz(f.ribs),
});
function summarize(b, full) {
  const { flat, tagged } = flat4(b.triangles);
  const pad = flat4(b.padTriangles ?? []);
  const fill = b.fill ? { ...b.fill, refused: Object.entries(b.fill.refused) } : null;
  return {
    nTriangles: b.triangles.length, hash: hashOf(flat), tagged, nPad: (b.padTriangles ?? []).length, padHash: hashOf(pad.flat), mode: b.mode,
    props: b.props.map(propOf), fins: b.fins.map(finOf), volume: nz(b.volume), skipped: { ...b.skipped }, rejected: { ...b.rejected },
    patchCount: b.patchCount, patchStats: { ...b.patchStats }, tines: nz(b.tines), servedRegions: nz(b.servedRegions), unserved: nz(b.unserved),
    sagRisk: nz(b.sagRisk), seating: b.seating, tip: nz(b.tip), braceCount: nz(b.braceCount), propCount: nz(b.propCount), fill,
    curvedFill: nz(b.curvedFill), curvedDropped: !!b.curvedDropped, sway: nz(b.sway), smallHoleRegions: nz(b.smallHoleRegions),
    smallHoleMask: b.smallHoleMask ?? null, tallRibs: nz(b.tallRibs), floating: nz(b.floating), bare: nz(b.bare), pad: b.pad ?? null,
    ...(full ? { triangles: flat } : {}),
  };
}

// ---- the recorder ------------------------------------------------------------------------------
const refs = new WeakMap();
const state = { model: "", scene: "", counts: {}, entries: [], soup: null, threshold: 45, full: false };
const plainOpts = (o) => {
  const out = {};
  for (const [k, v] of Object.entries(o ?? {})) {
    if (v === undefined || typeof v === "function") continue;
    out[k] = v;
  }
  return out;
};
globalThis.__sfFins = {
  call(name, fn, self, args) {
    if (name === "buildTopology") {
      const t = fn.apply(self, args);
      refs.set(t, new Ref({ $topo: state.model }));
      return t;
    }
    if (name === "analyze") {
      const r = fn.apply(self, args);
      const t = refs.get(args[0]);
      if (t) refs.set(r, new Ref({ $analysis: { model: t.payload.$topo, threshold: encode(args[1]), rot: encode(Array.from(args[2])) } }));
      return r;
    }
    if (name === "seatedPartTris") {
      const r = fn.apply(self, args);
      const t = refs.get(args[0]);
      if (t) refs.set(r, new Ref({ $seated: { model: t.payload.$topo, rot: encode(Array.from(args[1])), offset: encode(args[2]) } }));
      return r;
    }
    // buildFins
    const n = state.counts[state.scene] = (state.counts[state.scene] ?? -1) + 1;
    const opts = plainOpts(args[3]);
    const shown = state.soup
      ? [encode(state.soup), encode(state.threshold), encode(Array.from(args[2])), encode(opts)]
      : [encode(refs.get(args[0]) ?? args[0]), encode(refs.get(args[1]) ?? args[1]), encode(Array.from(args[2])), encode(opts)];
    const cfg = encode(snap());
    const r = fn.apply(self, args);
    state.entries.push({ id: `${state.scene}#${n}`, scene: state.scene, args: [...shown, { $config: cfg }], out: encode(summarize(r, state.full)) });
    return r;
  },
};

const T = await import(`${root}web/overhangs.js`);
const { sitePose, run, siteOpts } = await import(`${root}tests/_scene.js`);
const { buildFins } = await import(`${root}web/fins.js`);
const { MATERIAL } = await import(`${root}web/materials.js`);
const { readSTL } = await import(`${root}web/stl.js`);

function tunablesOf(material, extra) {
  const mat = MATERIAL[material];
  return { padH: mat.padH, padGrab: mat.padGrab, propGap: mat.propGap, padStyle: "auto", padCustom: { h: 0.5, gap: 0.0, grip: 0.05, margin: 4.0 }, cutout: "none", ...extra };
}
const IFACES = [false, "flat", "all"];

try {
  // ------------------------------------------------------------------ direct: models x poses x option sets (sampled)
  const MODELS = [...Deno.readDirSync(`${dir}/prototype/stress/models`)].filter((e) => e.name.endsWith(".stl")).map((e) => e.name.replace(/\.stl$/, "")).sort();
  const POSES = [[40, 0, 0], [0, 35, 20], [90, 0, 0]];
  const MODES = ["auto", "prop", "full", "draw", "auto", "prop"];
  const FULL_OK = new Set(["cube", "wedge", "lbracket", "lowledge", "ramp", "arch", "cone", "ushape", "tshape", "plus", "staircase", "portal"]);
  let k = 0;
  for (const name of MODELS) {
    state.model = name;
    const pos = readSTL(Deno.readFileSync(`${dir}/prototype/stress/models/${name}.stl`));
    const topo = T.buildTopology({ getAttribute: (kk) => (kk === "position" ? { array: pos } : null) });
    for (const deg of POSES) {
      const rot = sitePose(deg);
      const res = T.analyze(topo, 45, rot);
      for (let v = 0; v < 6; v++, k++) {
        let mode = MODES[(v + k) % MODES.length];
        if (mode === "full" && !FULL_OK.has(name)) mode = "auto";
        const material = (k >> 1) % 2 ? "petg" : "pla";
        const sway = (k >> 2) % 3 === 0;
        const iface = IFACES[(k + v) % 3];
        const nozzle = (k >> 3) % 2 ? 0.8 : 0.4;
        const wallLines = (k >> 4) % 2 ? 4 : 2;
        const opts = siteOpts({ material, mode, sway });
        opts.tunables = tunablesOf(material, { iface, nozzle, wallLines });
        if (v === 3) { opts.coverage = 0.9; opts.tineDensity = 0.5; opts.layerHeight = 0.24; }
        if (v === 4) { opts.plateOnly = true; opts.tallRibs = false; }
        if (v === 5 && mode !== "full") { opts.curvedWalls = true; }
        state.scene = `direct|${name}|${deg.join(",")}|${mode}|${material}|${sway ? "sway" : "nosway"}|${iface === false ? "off" : iface}|n${nozzle}|l${wallLines}|v${v}`;
        state.full = k % 53 === 0;
        buildFins(topo, res, rot, opts);
      }
    }
  }

  // Full coverage with curved walls: the judge keeps the curves only within CURVED.fullSlack
  for (const name of ["lbracket", "wedge", "lowledge", "arch", "ramp", "cone", "torus", "tube"]) {
    state.model = name;
    const pos = readSTL(Deno.readFileSync(`${dir}/prototype/stress/models/${name}.stl`));
    const topo = T.buildTopology({ getAttribute: (kk) => (kk === "position" ? { array: pos } : null) });
    for (const deg of POSES) {
      const rot = sitePose(deg);
      const res = T.analyze(topo, 45, rot);
      const opts = siteOpts({ material: "pla", mode: "full", sway: false });
      opts.tunables = tunablesOf("pla", { iface: false, nozzle: 0.4, wallLines: 2 });
      opts.curvedWalls = true;
      state.scene = `direct|${name}|${deg.join(",")}|full-curved`;
      state.full = false;
      buildFins(topo, res, rot, opts);
    }
  }

  // ------------------------------------------------------------------ direct: synthetic parts
  {
    const { block } = await import(`${root}tests/_util.js`);
    const { recessedBody } = await import(`${root}tests/_circular.js`);
    const { circularBore } = await import(`${root}tests/_bore.js`);
    const quad = (a, b, c, d) => [a, b, c, a, c, d];
    const flange = (n, R, z0, z1) => {
      const t = [], ring = (z) => Array.from({ length: n }, (_, i) => [R * Math.cos(2 * Math.PI * i / n), R * Math.sin(2 * Math.PI * i / n), z]);
      const lo = ring(z0), hi = ring(z1);
      for (let i = 0; i < n; i++) {
        const j = (i + 1) % n;
        t.push(lo[i], hi[j], lo[j], lo[i], hi[i], hi[j], [0, 0, z0], lo[j], lo[i], [0, 0, z1], hi[i], hi[j]);
      }
      return t.flat();
    };
    const pipe = (len, r, n, x0, y0, z0) => {
      const ring = (x) => Array.from({ length: n }, (_, i) => [x, y0 + r * Math.cos((2 * Math.PI * i) / n), z0 + r * Math.sin((2 * Math.PI * i) / n)]);
      const a = ring(x0), b = ring(x0 + len), t = [];
      for (let i = 0; i < n; i++) {
        const j = (i + 1) % n;
        t.push(...quad(a[i], b[i], b[j], a[j]), a[i], a[j], [x0, y0, z0], b[j], b[i], [x0 + len, y0, z0]);
      }
      return t.flat();
    };
    const SOUPS = {
      ledge: [...block(0, 28, -10, 0, 0, 27), ...block(0, 28, 0, 14, 25, 27)],
      "flange-hub": [...flange(64, 20, 20, 22), ...block(-5, 5, -5, 5, 0, 20.5)],
      "flange-hub-32": [...flange(32, 14, 12, 14), ...block(-4, 4, -4, 4, 0, 12.5)],
      deck: [...block(-20, 20, 40, 44, 0, 17), ...block(-20, 20, -44, -40, 0, 17), ...block(-20, 20, -40, 40, 15, 17)],
      recess: Array.from(recessedBody()),
      "recess-yaw": Array.from(recessedBody({ segments: 90, yaw: 7, height: 30, outer: 25, opening: 18, bore: 10, depth: 8 })),
      "recess-raised": Array.from(recessedBody({ translate: [3, -2, 12] })),
      bore: [...circularBore({ diameter: 5 }), ...block(30, 60, -10, 10, 10, 15)],
      "bore-wide": Array.from(circularBore({ diameter: 10, segments: 48 })),
      pipe: [...block(-40, 40, -20, 20, 0, 1), ...pipe(60, 10, 24, -30, 0, 20)],
      "pipe-on-post": [...block(-5, 5, -5, 5, 0, 20), ...pipe(30, 4, 16, -15, 0, 25)],
      "ledge-two-posts": [...block(-5, 5, -5, 5, 0, 20), ...block(25, 35, -5, 5, 0, 20), ...block(-5, 35, -8, 8, 20, 24)],
      "floating-slab": [...block(-5, 5, -5, 5, 0, 5), ...block(-12, 12, -10, 10, 15, 17)],
      "recess@70": Array.from(recessedBody()),
      "recess-yaw@70": Array.from(recessedBody({ segments: 90, yaw: 7, height: 30, outer: 25, opening: 18, bore: 10, depth: 8 })),
      "recess-small@70": Array.from(recessedBody({ segments: 60, height: 20, outer: 18, opening: 12, bore: 6, depth: 5 })),
      "bore@70": [...circularBore({ diameter: 5 }), ...block(30, 60, -10, 10, 10, 15)],
      "bore-wide@70": Array.from(circularBore({ diameter: 10, segments: 48 })),
      "two-ledges": [...block(0, 28, -10, 0, 0, 27), ...block(0, 28, 0, 14, 25, 27), ...block(40, 70, -10, 0, 0, 20), ...block(40, 70, 0, 18, 18, 20)],
    };
    // walls taller than BRACE.tallH (100 mm): the face ribs of fins/tall.js
    SOUPS["tall-ledge"] = [...block(0, 28, -10, 0, 0, 127), ...block(0, 28, 0, 14, 125, 127)];
    SOUPS["tall-shelf"] = [...block(-5, 5, -5, 5, 0, 120), ...block(25, 35, -5, 5, 0, 120), ...block(-5, 35, -8, 8, 120, 124)];
    for (const [name, soup] of Object.entries(SOUPS)) {
      if (name.includes("@")) continue;
      const pos = Float32Array.from(soup);
      state.soup = pos;
      state.threshold = 45;
      state.model = `soup-${name}`;
      const topo = T.buildTopology({ getAttribute: (kk) => (kk === "position" ? { array: pos } : null) });
      for (const deg of [[0, 0, 0], [40, 0, 0]]) {
        const rot = sitePose(deg);
        const res = T.analyze(topo, 45, rot);
        for (const mode of ["auto", "full", "prop"]) {
          if (name.startsWith("tall-") && mode === "prop") continue;
          const opts = siteOpts({ material: "pla", mode, sway: false });
          opts.tunables = tunablesOf("pla", { iface: false, nozzle: 0.4, wallLines: 2 });
          state.scene = `soup|${name}|${deg.join(",")}|${mode}`;
          state.full = false;
          buildFins(topo, res, rot, opts);
        }
      }
    }
    state.soup = null;
  }

  // ------------------------------------------------------------------ captured: the 54 golden scenes
  state.full = false;
  for (const scene of goldenScenes()) {
    state.scene = scene.name;
    state.model = scene.model;
    run(scene);
  }
  const direct = state.entries.filter((e) => e.scene.startsWith("direct|") || e.scene.startsWith("soup|"));
  const captured = state.entries.filter((e) => !e.scene.startsWith("direct|") && !e.scene.startsWith("soup|"));
  writeCases("buildfins", "buildFins", direct, { encoded: true });
  writeCases("buildfins", "buildFins.captured", captured, { encoded: true });
  console.log("buildfins: direct", direct.length, "captured", captured.length);
} finally {
  Deno.removeSync(dir, { recursive: true });
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout was modified");
