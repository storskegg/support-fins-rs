// Records parity/corpus/build/{buildProps,buildShortWalls}.jsonl (direct) and *.captured.jsonl
// (the calls the 54 golden scenes' builds make) from web/prop.js: THE MILESTONE'S EXIT TEST.
//
//   buildProps       args: [$topo, result, rot, opts, crest, {$config: {PROP}}]
//   buildShortWalls  args: [$topo, result, rot, opts, regions, crest, {$config: {PROP}}]
//     result  a `$analysis` reference, or (the captured builds') the augmented copy
//             fins/coverage.js makes, recorded in full (its `excludedSupportFaces` is read by the
//             curved-walls trace)
//     opts    the keys the prop pipeline reads, as plain data: step, coverage, tines, tineDensity,
//             layerHeight, raster, sway ({on}), rimWalls, downhill, curvedWalls, plateOnly, endRows;
//             `rasterVeto` (a function in the captured builds: fins/wedges.js wedgeVeto) is recorded as
//             `rasterVetoLog`, the booleans it answered in call order, which the replay plays back
//     crest   { part: $seated | null, layerHeight }: prop/crest.js's module state at the call
//     tineCap globalThis.__TINECAP at the call: null (unset) or the seeds already in it, [[x, y, z, biteX, biteY]...]
//             (Auto's downhill judge sets it); the out summary's `cap` is its state after the call
//     out     a SUMMARY of the pass: { nTriangles, hash: [h1, h2], tagged, props, skipped, served,
//             servedRegions, tines, sagRisk, sagRegions, leaderOf, volume, triangles? }. `hash` is two
//             32-bit FNV-style hashes over the little-endian words of every vertex as 4 float64s
//             (x, y, z, tag); the direct cases flagged `full` also carry `triangles` (Float64Array,
//             4 per vertex). A prop is { region, id, tines, caps, grip, span, height, area, stations,
//             trimmed, volume, line, triRanges, triRange, wallRange, members, rimWall, partAttached,
//             inBore, squat, tags: [raster, circular, rim, downhill, curved, flush] } with null for a key
//             the wall does not have. `cap` is null (no capture) or { n, hash } of the capture after the call.
// Replayed by crates/sf-testkit/tests/parity_build.rs with Tol::Exact.
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
const dir = Deno.makeTempDirSync({ prefix: "sf-build-" });
copyTree(`${UPSTREAM_ROOT}/web`, `${dir}/web`);
Deno.mkdirSync(`${dir}/tests`, { recursive: true });
for (const f of ["_util.js", "_scene.js", "_circular.js", "_bore.js"]) Deno.copyFileSync(`${UPSTREAM_ROOT}/tests/${f}`, `${dir}/tests/${f}`);
copyTree(`${UPSTREAM_ROOT}/prototype/stress/models`, `${dir}/prototype/stress/models`);
copyTree(`${UPSTREAM_ROOT}/tests/fixtures`, `${dir}/tests/fixtures`);

const FNS = { "overhangs.js": ["buildTopology", "analyze"], "fins/seating.js": ["seatedPartTris"], "prop/crest.js": ["crestPart"], "prop.js": ["buildProps", "buildShortWalls"] };
for (const [rel, list] of Object.entries(FNS)) {
  const file = `${dir}/web/${rel}`;
  let src = Deno.readTextFileSync(file);
  for (const f of list) {
    const re = new RegExp(`^export function ${f}\\(`, "gm");
    if ((src.match(re) ?? []).length !== 1) throw new Error(`build: expected one 'export function ${f}(' in ${rel}`);
    src = src.replace(re, `function __${f}(`);
  }
  src = `const __rec = (name, fn) => function (...args) { return globalThis.__sfBuild.call(name, fn, this, args); };\n${src}\n`
    + list.map((f) => `export const ${f} = __rec('${f}', __${f});`).join("\n") + "\n";
  Deno.writeTextFileSync(file, src);
}

const root = `file://${dir}/`;
const propCfg = await import(`${root}web/prop/config.js`);
const { PROP } = propCfg;

// ---- the recorder ------------------------------------------------------------------------------
const refs = new WeakMap();
const state = { model: "", scene: "", counts: {}, crest: { part: null, layerHeight: undefined }, entries: { buildProps: [], buildShortWalls: [] } };
const TAGS = ["raster", "circular", "rim", "downhill", "curved", "flush"];

const seedsOf = (cap) => (cap ? cap.map((c) => [c.x, c.y, c.z, c.biteX, c.biteY]) : null);
function capSummary(cap) {
  if (!cap) return null;
  const flat = Float64Array.from(cap.flatMap((c) => [c.x, c.y, c.z, c.biteX, c.biteY]));
  const w = new Uint32Array(flat.buffer);
  let h1 = 0x811c9dc5, h2 = 0x9747b28c;
  for (let i = 0; i < w.length; i++) { h1 = Math.imul(h1 ^ w[i], 16777619); h2 = Math.imul(h2 ^ w[i], 0x01000193) ^ (h2 >>> 13); }
  return { n: cap.length, hash: [h1 >>> 0, h2 >>> 0] };
}

function summarize(r, full) {
  const n = r.triangles.length;
  const flat = new Float64Array(n * 4);
  let tagged = 0;
  for (let i = 0; i < n; i++) {
    const v = r.triangles[i];
    flat[i * 4] = v[0]; flat[i * 4 + 1] = v[1]; flat[i * 4 + 2] = v[2];
    if (v[3] === 1) { flat[i * 4 + 3] = 1; tagged++; }
  }
  const w = new Uint32Array(flat.buffer);
  let h1 = 0x811c9dc5, h2 = 0x9747b28c;
  for (let i = 0; i < w.length; i++) { h1 = Math.imul(h1 ^ w[i], 16777619); h2 = Math.imul(h2 ^ w[i], 0x01000193) ^ (h2 >>> 13); }
  const nz = (x) => (x === undefined ? null : x);
  const props = r.props.map((q) => ({
    region: q.region, id: nz(q.id), tines: nz(q.tines), caps: nz(q.caps), grip: nz(q.grip), span: nz(q.span), height: nz(q.height),
    area: nz(q.area), stations: nz(q.stations), trimmed: nz(q.trimmed), volume: nz(q.volume), line: nz(q.line), triRanges: nz(q.triRanges),
    triRange: nz(q.triRange), wallRange: nz(q.wallRange), members: nz(q.members), rimWall: !!q.rimWall, partAttached: !!q.partAttached,
    inBore: nz(q.inBore), squat: !!q.squat, tags: TAGS.map((t) => nz(q[t])),
  }));
  return {
    nTriangles: n, hash: [h1 >>> 0, h2 >>> 0], tagged, props, skipped: { ...r.skipped }, served: r.served, servedRegions: nz(r.servedRegions),
    tines: nz(r.tines), sagRisk: nz(r.sagRisk), sagRegions: r.sagRegions ? [...r.sagRegions] : null,
    leaderOf: r.leaderOf ? [...r.leaderOf] : null, volume: r.volume, ...(full ? { triangles: flat } : {}),
  };
}

const PLAIN = ["step", "coverage", "tines", "tineDensity", "layerHeight", "raster", "rimWalls", "downhill", "curvedWalls", "plateOnly", "shortWalls", "endRows"];
const crestArg = () => ({ part: state.crest.part, layerHeight: state.crest.layerHeight === undefined ? { $u: 1 } : encode(state.crest.layerHeight) });
globalThis.__sfBuild = {
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
    if (name === "crestPart") {
      const part = args[0];
      state.crest = { part: part ? encode(state.soup ? part : (refs.get(part) ?? part)) : null, layerHeight: args[1] };
      return fn.apply(self, args);
    }
    // buildProps / buildShortWalls
    const isShort = name === "buildShortWalls";
    const opts = args[3] ?? {};
    const plain = {};
    for (const k of PLAIN) if (opts[k] !== undefined) plain[k] = opts[k];
    if (opts.sway !== undefined) plain.sway = { on: !!opts.sway?.on };
    const log = [];
    const call = [...args];
    if (opts.rasterVeto) call[3] = { ...opts, rasterVeto: (...a) => { const v = !!opts.rasterVeto(...a); log.push(v); return v; } };
    const shown = state.soup
      ? [encode(state.soup), encode(state.threshold), encode(Array.from(args[2])), encode(plain)]
      : [encode(refs.get(args[0]) ?? args[0]), encode(refs.get(args[1]) ?? args[1]), encode(Array.from(args[2])), encode(plain)];
    if (isShort) shown.push(encode([...args[4]]));
    const n = state.counts[`${state.scene}\0${name}`] = (state.counts[`${state.scene}\0${name}`] ?? -1) + 1;
    const entry = { scene: state.scene, n, args: shown, crest: crestArg(), cfg: { PROP: encode({ ...PROP }) }, plain };
    entry.tineCap = encode(seedsOf(globalThis.__TINECAP));
    const r = fn.apply(self, call);
    if (opts.rasterVeto) entry.plain = { ...plain, rasterVetoLog: log };
    entry.out = summarize(r, !!state.full);
    entry.out.cap = capSummary(globalThis.__TINECAP);
    state.entries[name].push(entry);
    return r;
  },
};

const T = await import(`${root}web/overhangs.js`);
const S = await import(`${root}web/fins/seating.js`);
const C = await import(`${root}web/prop/crest.js`);
const P = await import(`${root}web/prop.js`);
const { sitePose, run } = await import(`${root}tests/_scene.js`);
const { readSTL } = await import(`${root}web/stl.js`);

const caseOf = (e, fn) => {
  const a = [...e.args];
  const opts = encode(e.plain);
  a[3] = opts;
  return { id: `${e.scene}#${e.n}`, args: [...a, e.crest, e.tineCap, { $config: e.cfg }], out: encode(e.out) };
};

try {
  // ------------------------------------------------------------------ direct: models x poses x option sets
  const MODELS = [...Deno.readDirSync(`${dir}/prototype/stress/models`)].filter((e) => e.name.endsWith(".stl")).map((e) => e.name.replace(/\.stl$/, "")).sort();
  const POSES = [[40, 0, 0], [0, 35, 20], [90, 0, 0]];
  const SETS = {
    noraster: { tines: true, raster: false },
    default: { tines: true },
    downhill: { tines: true, downhill: true },
    curved: { tines: true, downhill: true, curvedWalls: true },
    norim: { tines: true, rimWalls: false },
    noendrows: { tines: true, endRows: false },   // the flush layout everywhere, no race
    sparse: { coverage: 0.2, tineDensity: 0.5, layerHeight: 0.24 },
  };
  const FULL = new Set(["cube|40,0,0|noraster|off", "wedge|40,0,0|default|off", "lowledge|0,35,20|curved|flat", "ramp|90,0,0|norim|flat"]);
  const wasIface = PROP.iface;
  for (const name of MODELS) {
    state.model = name;
    const pos = readSTL(Deno.readFileSync(`${dir}/prototype/stress/models/${name}.stl`));
    const topo = T.buildTopology({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });
    for (const deg of POSES) {
      const rot = sitePose(deg);
      const res = T.analyze(topo, 45, rot);
      if (!res.regions.length) continue;
      const soup = S.seatedPartTris(topo, rot, res.offset);
      for (const [sname, sopts] of Object.entries(SETS)) {
        for (const iface of ["off", "flat"]) {
          PROP.iface = iface === "flat" ? "flat" : false;
          C.crestPart(iface === "flat" ? soup : null, sopts.layerHeight ?? 0.2);
          state.scene = `direct|${name}|${deg.join(",")}|${sname}|${iface}`;
          state.full = FULL.has(`${name}|${deg.join(",")}|${sname}|${iface}`);
          P.buildProps(topo, res, rot, { ...sopts });
          if (sname === "default" && iface === "off") {
            // the tests' tine capture on (and holding seeds from before the build)
            globalThis.__TINECAP = [{ x: 1, y: 2, z: 3, biteX: 4, biteY: 5 }];
            state.scene += "|cap";
            P.buildProps(topo, res, rot, { ...sopts });
            globalThis.__TINECAP = undefined;
          }
        }
      }
      // the last-resort pass over the first region
      PROP.iface = false; C.crestPart(null);
      state.scene = `direct|${name}|${deg.join(",")}|short`; state.full = false;
      P.buildShortWalls(topo, res, rot, { tines: true }, new Set([0]));
    }
  }
  PROP.iface = wasIface; C.crestPart(null);

  // ------------------------------------------------------------------ direct: synthetic parts (round flange, rim ledge, circular recess, ...)
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
    for (const [name, soup] of Object.entries(SOUPS)) {
      const threshold = name.endsWith("@70") ? 70 : 45;
      state.threshold = threshold;
      const pos = Float32Array.from(soup);
      state.soup = pos;
      state.model = `soup-${name}`;
      const topo = T.buildTopology({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });
      for (const deg of [[0, 0, 0], [40, 0, 0]]) {
        const rot = sitePose(deg);
        const res = T.analyze(topo, threshold, rot);
        if (!res.regions.length) continue;
        const sp = S.seatedPartTris(topo, rot, res.offset);
        for (const [sname, sopts] of Object.entries(SETS)) {
          if (sname === "sparse") continue;
          for (const iface of ["off", "flat"]) {
            if (iface === "flat" && sname !== "default" && sname !== "downhill") continue;
            PROP.iface = iface === "flat" ? "flat" : false;
            C.crestPart(iface === "flat" ? sp : null, 0.2);
            state.scene = `soup|${name}|${deg.join(",")}|${sname}|${iface}`;
            state.full = false;
            P.buildProps(topo, res, rot, { ...sopts });
          }
        }
      }
    }
    state.soup = null;
    PROP.iface = wasIface; C.crestPart(null);
  }

  // ------------------------------------------------------------------ captured: the 54 golden scenes
  state.full = false;
  for (const scene of goldenScenes()) {
    state.scene = scene.name;
    state.model = scene.model;
    PROP.iface = wasIface;
    run(scene);
  }
  const done = (fn) => state.entries[fn].filter((e) => e.scene.startsWith("direct|")).map((e) => caseOf(e, fn));
  const soups = state.entries.buildProps.filter((e) => e.scene.startsWith("soup|")).map((e) => caseOf(e, "buildProps"));
  writeCases("build", "buildProps.soup", soups, { encoded: true });
  const cap = (fn) => state.entries[fn].filter((e) => !e.scene.startsWith("direct|") && !e.scene.startsWith("soup|")).map((e) => caseOf(e, fn));
  for (const fn of ["buildProps", "buildShortWalls"]) {
    writeCases("build", fn, done(fn), { encoded: true });
    writeCases("build", `${fn}.captured`, cap(fn), { encoded: true });
  }
  console.log("build: direct", state.entries.buildProps.filter((e) => e.scene.startsWith("direct|")).length, "captured",
    state.entries.buildProps.filter((e) => !e.scene.startsWith("direct|")).length, "short", state.entries.buildShortWalls.length);
} finally {
  Deno.removeSync(dir, { recursive: true });
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout was modified");
