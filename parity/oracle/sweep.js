// Oracle recorder for web/prop/sweep.js -> parity/corpus/sweep/<fn>.jsonl. Replayed in
// crates/sf-testkit/tests/parity_sweep.rs with Tol::Exact.
//
// Calls come from two sources, tagged by scene in each case id:
//   direct-*  synthetic lines (flat, sloped, wavy, curved) x config variants (rounded feet,
//             vertical pins, cutout patterns, interface crest modes over real undersides,
//             nozzle widths), and grids for the three small helpers;
//   build-*   the real calls of the 54 golden scenes' builds, plus builds of a few scenes with
//             the interface crest, a cutout pattern and vertical pins switched on.
// Every case ends with the config snapshot ({"$config": {PROP, CUT, RESIN_FOOT}} for sweep,
// {PROP} for the helpers).
//   foot_for / stem_half   args: [h | rise, $config]                       out: number
//   profile_half           args: [z, top, $config]                         out: number
//   sweep                  args: [line, zBed, minH, state, $config]        out: {ret, tris}
//   sweep_between          args: [topLine, botLine, minH, state, $config]  out: {ret, tris}
// `state` is the crest's module state {part, layerHeight}: `part` is a {"$seated": ...} reference
// (builds), a full Float64Array (direct), or null. `tris` is a Float64Array of 4 numbers per
// vertex (x, y, z, tag) the call pushed onto its `out` array: tag 1 where JS has `v[3] === 1`.
// The recorder wraps sweep.js itself (rather than via captureModules) because it must read
// `out` AFTER the call; the standard references still come from captureModules.
import { assertPinned, UPSTREAM_ROOT } from './lib/pin.js';
import { captureModules, setScene, upstreamStatus } from './lib/capture.js';
import { encode, writeCases } from './lib/encode.js';

assertPinned();
const statusBefore = upstreamStatus();

function rng(seed) {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13; s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5; s >>>= 0;
    return s / 4294967296;
  };
}
const R = rng(0x5eee9b11);
const rnd = (lo, hi) => lo + R() * (hi - lo);

// --- the captured tree: references for topologies / analyses / seated parts, crestPart's args ----
const cap = await captureModules(
  [
    { rel: 'overhangs.js', fns: ['buildTopology', 'analyze'] },
    { rel: 'fins/seating.js', fns: ['seatedPartTris'] },
    { rel: 'prop/crest.js', fns: ['crestPart'] },
  ],
  {
    cap: 1e9,
    refs: true,
    copy: ['tests/_util.js', 'tests/_scene.js', 'prototype/stress/models/'],
    args: {
      buildTopology: ([g]) => [g.getAttribute('position').array],
      analyze: ([, th, rot], sink) => [sink.model, th, rot],
    },
  },
);

// --- wrap prop/sweep.js in the tree: record args, config and what a call pushed ----------------
const sweepFile = `${cap.root}/web/prop/sweep.js`;
let src = Deno.readTextFileSync(sweepFile);
src = src.replace('export const footFor = (h) => {', 'const __footFor = (h) => {');
for (const f of ['profileHalf', 'stemHalf', 'sweep', 'sweepBetween']) {
  const re = new RegExp(`^export function ${f}\\(`, 'm');
  if (!re.test(src)) throw new Error(`sweep.js: no 'export function ${f}('`);
  src = src.replace(re, `function __${f}(`);
}
src = `const __rec = (name, fn) => function (...args) { return globalThis.__sfSweep(name, fn, this, args); };\n${src}\n`
  + ['footFor', 'profileHalf', 'stemHalf', 'sweep', 'sweepBetween'].map((f) => `export const ${f} = __rec('${f}', __${f});`).join('\n') + '\n';
Deno.writeTextFileSync(sweepFile, src);

const propMod = await import(`${cap.rootUrl}web/prop/config.js`);
const cutMod = await import(`${cap.rootUrl}web/cutout.js`);
const footMod = await import(`${cap.rootUrl}web/prop/resinfoot.js`);
const nozzleMod = await import(`${cap.rootUrl}web/nozzle.js`);
const crestMod = cap.modules['prop/crest.js'];
const { PROP } = propMod, { CUT } = cutMod, { RESIN_FOOT } = footMod;

const log = { footFor: [], profileHalf: [], stemHalf: [], sweep: [], sweepBetween: [] };
const counts = new Map();
const flat4 = (vs) => Float64Array.from(vs.flatMap((v) => [v[0], v[1], v[2], v[3] === 1 ? 1 : 0]));
globalThis.__sfSweep = (name, fn, self, args) => {
  const tag = cap.sink.tag;
  const key = `${tag}\0${name}`;
  const n = counts.get(key) ?? 0;
  counts.set(key, n + 1);
  const isSweep = name === 'sweep' || name === 'sweepBetween';
  const entry = {
    scene: tag, n, cp: cap.sink.counts.crestPart ?? 0,
    args: (isSweep ? args.slice(0, 2).concat(args.slice(3)) : args).map(encode), // `out` is not an input
    cfg: isSweep ? { PROP: encode({ ...PROP }), CUT: encode({ ...CUT }), RESIN_FOOT: encode({ ...RESIN_FOOT }) } : { PROP: encode({ ...PROP }) },
  };
  const out = isSweep ? args[isSweep && name === 'sweep' ? 2 : 2] : null;
  const from = out?.length;
  const ret = fn.apply(self, args);
  entry.ret = ret;
  if (out) entry.tris = flat4(out.slice(from));
  log[name].push(entry);
  return ret;
};

const { sweep, sweepBetween, footFor, profileHalf, stemHalf } = await import(`${cap.rootUrl}web/prop/sweep.js`);

/** Sets fields on a config object for the duration of `fn`. */
function withVals(obj, vals, fn) {
  const was = {};
  for (const k of Object.keys(vals)) was[k] = obj[k];
  Object.assign(obj, vals);
  try { return fn(); } finally { Object.assign(obj, was); }
}

function block(x0, x1, y0, y1, z0, z1) {
  const v = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
  const t = [];
  for (const [a, b, c] of [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]]) t.push(...v[a], ...v[b], ...v[c]);
  return t;
}
function tilted(deg, z0) {
  const a = (deg * Math.PI) / 180, c = Math.cos(a), s = Math.sin(a);
  const t = block(-40, 40, -20, 20, 0, 4);
  for (let i = 0; i < t.length; i += 3) {
    const x = t[i], z = t[i + 2];
    t[i] = c * x + s * z; t[i + 2] = -s * x + c * z + z0;
  }
  return Float64Array.from(t);
}

try {
  // --- direct: the three helpers --------------------------------------------------------------
  setScene(cap.sink, 'direct-helpers');
  crestMod.crestPart(null);
  const helperVariants = [{}, { th: 2.4, tip: 1.5 }, { neckDeg: 30 }, { footMin: 3, footMax: 2 }, { maxUnsupportedSpan: 4 }, { tipH: 4, baseH: 1.2 }, { footRatio: 0.5 }];
  for (const v of helperVariants) {
    withVals(PROP, v, () => {
      for (const h of [-1, 0, 0.5, 1.5, 3, 8, 14, 25, 60, 200, NaN]) footFor(h);
      for (const r of [-1, 0, 0.05, 0.2, 0.7, 1.5, 3, 10, NaN]) stemHalf(r);
      for (const top of [0.3, 1, 2, 5, 12, 40]) for (const z of [0, 0.3, 0.6, 0.7, 1, 2.5, 6, 11.9, 12, 40, 41]) profileHalf(z, top);
    });
  }
  // nozzle-scaled walls
  for (const [d, n] of [[0.2, 2], [0.6, 2], [0.8, 4], [1.0, 6], [2.0, 8]]) {
    nozzleMod.applyNozzle(d); nozzleMod.applyWallLines(n);
    for (const h of [3, 14, 60]) footFor(h);
    for (const r of [0.1, 0.5, 2]) stemHalf(r);
    for (const top of [1, 5, 20]) for (const z of [0, 1, 4, 19]) profileHalf(z, top);
  }
  nozzleMod.applyNozzle(0.4); nozzleMod.applyWallLines(2);

  // --- direct: sweep / sweepBetween ----------------------------------------------------------
  const mk = (L, step, zOf, yOf = () => 0) => {
    const l = [];
    for (let x = -L / 2; x <= L / 2 + 1e-9; x += step) l.push([x, yOf(x), zOf(x)]);
    return l;
  };
  const lines = [];
  for (const L of [6, 14, 30]) for (const Z of [3, 12, 40]) {
    lines.push([`flat-L${L}-Z${Z}`, mk(L, 1, () => Z)]);
    lines.push([`slope-L${L}-Z${Z}`, mk(L, 1, (x) => Z - 0.4 * x)]);
    lines.push([`wavy-L${L}-Z${Z}`, mk(L, 1, (x) => Z + 2 * Math.sin(x / 4))]);
    lines.push([`curve-L${L}-Z${Z}`, mk(L, 1, () => Z, (x) => 0.02 * x * x)]);
  }
  lines.push(['two', [[0, 0, 10], [4, 0, 10]]], ['one', [[0, 0, 10]]], ['repeat', [[0, 0, 10], [0, 0, 10], [3, 0, 10]]]);
  lines.push(['low', mk(8, 1, () => 1.2)], ['tail', mk(8, 1, (x) => 1.0 + 0.8 * (x + 4))], ['vertical', [[0, 0, 5], [0, 0.0000001, 9]]]);
  const run = (tag, f) => { setScene(cap.sink, tag); crestMod.crestPart(null); f(); };
  const variants = [
    ['plain', {}], ['round', { roundFeet: true }], ['pins', { tineDir: 'vertical' }], ['pins-round', { tineDir: 'vertical', roundFeet: true }],
    ['thick', { th: 3, tip: 1.2 }], ['gap', { gap: 0.5, baseH: 1 }], ['neck', { neckDeg: 30, tipH: 3 }],
  ];
  lines.forEach(([name, line], i) => {
    variants.forEach(([vn, vals], vi) => {
      if ((i + vi) % 3) return;
      run(`direct-sweep-${vn}-${name}`, () => withVals(PROP, vals, () => sweep(line, 0, [], PROP.minHeight)));
    });
    run(`direct-sweep-minH-${name}`, () => sweep(line, 0, [], 6));
    run(`direct-sweep-bed-${name}`, () => sweep(line, 2, [], PROP.minHeightSquat));
  });
  // with a cutout pattern
  lines.filter((_, i) => i % 4 === 0).forEach(([name, line]) => {
    for (const pat of ['diamond', 'lattice', 'honeycomb']) {
      run(`direct-sweep-cut-${pat}-${name}`, () => withVals(CUT, { pattern: pat }, () => sweep(line, 0, [], PROP.minHeight)));
    }
  });
  // with the interface crest, over undersides that are flat / nearly / steep
  const parts = [['flat', Float64Array.from(block(-40, 40, -20, 20, 12, 16)), 12], ['t5', tilted(5, 14), 14], ['t35', tilted(35, 20), 20]];
  for (const [pn, part, z0] of parts) {
    for (const iface of ['flat', 'all']) {
      for (const lh of [undefined, 0.2]) {
        const line = mk(14, 1, (x) => (pn === 'flat' ? 12 : pn === 't5' ? z0 - Math.tan(5 * Math.PI / 180) * x : z0 - Math.tan(35 * Math.PI / 180) * x));
        setScene(cap.sink, `direct-sweep-crest-${iface}-${pn}-lh${lh}`);
        crestMod.crestPart(part, lh);
        withVals(PROP, { iface }, () => sweep(line, 0, [], PROP.minHeight));
        withVals(PROP, { iface, ifaceLayers: 3 }, () => sweep(line, 0, [], PROP.minHeight));
        const floor = line.map(([x, y]) => [x, y, 2 + 0.05 * x, 1.5, 2.5]);
        withVals(PROP, { iface }, () => sweepBetween(line, floor, [], PROP.minHeight));
        crestMod.crestPart(null);
      }
    }
  }
  // sweepBetween: floors with and without side floors, welded or lifted
  lines.filter((_, i) => i % 3 === 1).forEach(([name, line]) => {
    const floors = {
      plate: line.map(([x, y]) => [x, y, 0]),
      level: line.map(([x, y]) => [x, y, 2]),
      slope: line.map(([x, y]) => [x, y, 1 + 0.06 * (x + 8)]),
      sides: line.map(([x, y]) => [x, y, 2 + 0.05 * x, 1.4 + 0.05 * x, 2.6 + 0.05 * x]),
      mixed: line.map(([x, y], k) => (k % 2 ? [x, y, 0] : [x, y, 3, 2.5, 3.5])),
    };
    for (const [fn_, floor] of Object.entries(floors)) {
      for (const footGap of [0.2, 0]) {
        run(`direct-between-${fn_}-fg${footGap}-${name}`, () => withVals(PROP, { footGap }, () => sweepBetween(line, floor, [], PROP.minHeight)));
      }
    }
    run(`direct-between-squat-${name}`, () => sweepBetween(line, floors.level, [], PROP.minHeightSquat));
    run(`direct-between-cut-${name}`, () => withVals(CUT, { pattern: 'lattice' }, () => sweepBetween(line, floors.sides, [], PROP.minHeight)));
  });

  // --- the golden scenes' own builds ------------------------------------------------------------
  const { siteOpts, sitePose, run: runScene } = await import(`${cap.rootUrl}tests/_scene.js`);
  const util = await import(`${cap.rootUrl}tests/_util.js`);
  const goldenDir = `${UPSTREAM_ROOT}/tests/golden`;
  const scenes = [...Deno.readDirSync(goldenDir)].filter((e) => e.name.endsWith('.json')).map((e) => e.name).sort()
    .map((n) => JSON.parse(Deno.readTextFileSync(`${goldenDir}/${n}`)).scene);
  for (const scene of scenes) {
    setScene(cap.sink, `build-${scene.name}`);
    cap.sink.model = scene.model;
    runScene(scene);
  }
  const picked = ['cube-x40-auto', 'lbracket-x35-auto', 'stress-ramp-x40', 'stress-wedge-up', 'stress-tshape-x40', 'stress-portal-x40'];
  const extra = [['flat', { iface: 'flat' }], ['all', { iface: 'all' }], ['lattice', { cutout: 'lattice' }], ['pins', { tineDir: 'vertical' }]];
  for (const scene of scenes.filter((s) => !s.draw && !s.set?.sway && picked.some((p) => s.name.startsWith(p)))) {
    cap.sink.model = scene.model; // the topology's reference is the model id current when it is built
    const rot = sitePose(scene.rot);
    const topo = util.loadModel(scene.model);
    const res = util.analyze(topo, 45, rot);
    for (const [label, tun] of extra) {
      setScene(cap.sink, `build-${scene.name}-${label}`);
      cap.sink.model = scene.model;
      const opts = siteOpts(scene.set);
      opts.layerHeight = 0.2;
      opts.tunables = { ...opts.tunables, ...tun };
      util.fins.buildFins(topo, res, rot, opts);
    }
    util.fins.applyTunables({ cutout: 'none', tineDir: 'horizontal', iface: false, pinDepth: 0.1 });
  }

  // --- assemble cases ----------------------------------------------------------------------------
  const cpLog = new Map(); // scene -> [crestPart call args (encoded, references substituted)]
  for (const e of cap.sink.calls.crestPart.entries()) {
    if (!cpLog.has(e.scene)) cpLog.set(e.scene, []);
    cpLog.get(e.scene)[e.n] = e.args;
  }
  const stateOf = (e) => {
    const a = e.cp > 0 ? cpLog.get(e.scene)?.[e.cp - 1] : null;
    return a ? { part: a[0], layerHeight: a[1] ?? { $u: 1 } } : { part: null, layerHeight: { $u: 1 } };
  };
  const spread = (calls, n) => (calls.length <= n ? calls : Array.from({ length: n }, (_, i) => calls[Math.floor((i * calls.length) / n)]));
  const finish = (fn, entries, make) => writeCases('sweep', fn, spread(entries, make.cap).map((e) => ({
    id: `${e.scene}#${e.n}`, args: make.args(e), out: make.out(e),
  })), { encoded: true });
  const cfgArg = (e, names) => ({ $config: Object.fromEntries(names.map((k) => [k, e.cfg[k]])) });
  const rt = (e) => ({ ret: e.ret, tris: encode(e.tris) });
  finish('foot_for', log.footFor, { cap: 400, args: (e) => [...e.args, cfgArg(e, ['PROP'])], out: (e) => encode(e.ret) });
  finish('stem_half', log.stemHalf, { cap: 400, args: (e) => [...e.args, cfgArg(e, ['PROP'])], out: (e) => encode(e.ret) });
  finish('profile_half', log.profileHalf, { cap: 500, args: (e) => [...e.args, cfgArg(e, ['PROP'])], out: (e) => encode(e.ret) });
  finish('sweep', log.sweep, { cap: 150, args: (e) => [...e.args, stateOf(e), cfgArg(e, ['PROP', 'CUT', 'RESIN_FOOT'])], out: rt });
  finish('sweep_between', log.sweepBetween, { cap: 80, args: (e) => [...e.args, stateOf(e), cfgArg(e, ['PROP', 'CUT', 'RESIN_FOOT'])], out: rt });
  console.log('sweep: calls', JSON.stringify(Object.fromEntries(Object.entries(log).map(([k, v]) => [k, v.length]))));
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error('the upstream checkout changed during the run');
