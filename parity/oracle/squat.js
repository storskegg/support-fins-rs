// Oracle recorder for web/prop/squat.js (sweepSquat, buildSquatBed) -> parity/corpus/squat/
// <fn>.captured.jsonl. Replayed in crates/sf-testkit/tests/parity_squat.rs with Tol::Exact.
//
// Calls come from direct-* (synthetic low lines through sweepSquat with config variants, and
// lines laid on the lowest surface of real stress models through buildSquatBed) and build-*
// (the 54 golden scenes' own calls, plus builds with the crest and vertical pins on). Every
// case ends with a config snapshot {"$config": {PROP, RESIN_FOOT}}; `state` is the crest's
// module state {part, layerHeight} (a `$seated` reference in builds, a full Float64Array in
// direct cases, or null), placed before the config.
//   sweepSquat      args: [line, zBed, [] (the `out` argument), inset|null, state, $config]
//                   out:  {ret, tris}
//   buildSquatBed   args: [line, regionTris, topo, rot, offset, [] (`out`), claimed|null, state, $config]
//                   out:  {from (out.length at the call: `triRange` is absolute), placed: [{span, height, stations, volume, squat, line, triRange}],
//                          tris}
// `tris` is a Float64Array of 4 numbers per vertex (x, y, z, tag) pushed onto `out` and still
// in it when the call returned (buildSquatBed truncates a rejected wall).
import { assertPinned, UPSTREAM_ROOT } from './lib/pin.js';
import { captureModules, setScene, upstreamStatus } from './lib/capture.js';
import { encode, writeCases } from './lib/encode.js';

assertPinned();
const statusBefore = upstreamStatus();

const recs = new Map(); // `${fn}:${scene}#${n}` -> {rec, cp}
// `out` is replaced by a proxy that forwards to the real array while recording what the call
// leaves in it: pushes append to `rec`, an indexed write (dropCollapsed's in-place compaction)
// mirrors, and a truncation (`out.length = before`) cuts it.
const recordOut = (fn, outIndex) => (args, sink) => {
  const real = args[outIndex];
  const from = real.length;
  const rec = [];
  args[outIndex] = new Proxy(real, {
    get(t, k) {
      if (k === 'push') return (...v) => { rec.push(...v); return t.push(...v); };
      const x = Reflect.get(t, k);
      return typeof x === 'function' ? x.bind(t) : x;
    },
    set(t, k, v) {
      if (k === 'length') rec.length = Math.max(0, v - from);
      else if (typeof k === 'string' && /^\d+$/.test(k) && Number(k) >= from) rec[Number(k) - from] = v; // dropCollapsed compacts in place
      t[k] = v;
      return true;
    },
  });
  recs.set(`${fn}:${sink.tag}#${sink.counts[fn] - 1}`, { rec, from, cp: sink.counts.crestPart ?? 0 });
  // the recorded `out` argument is the empty array it was at the call
  const shown = [...args];
  shown[outIndex] = [];
  return shown;
};

let propMod, footMod;
const cap = await captureModules(
  [
    { rel: 'overhangs.js', fns: ['buildTopology', 'analyze'] },
    { rel: 'fins/seating.js', fns: ['seatedPartTris'] },
    { rel: 'prop/crest.js', fns: ['crestPart'] },
    { rel: 'prop/squat.js', fns: ['sweepSquat', 'buildSquatBed'] },
  ],
  {
    cap: 1e9,
    refs: true,
    copy: ['tests/_util.js', 'tests/_scene.js', 'prototype/stress/models/'],
    args: {
      buildTopology: ([g]) => [g.getAttribute('position').array],
      analyze: ([, th, rot], sink) => [sink.model, th, rot],
      sweepSquat: recordOut('sweepSquat', 2),
      buildSquatBed: recordOut('buildSquatBed', 5),
    },
    config: { sweepSquat: ['PROP', 'RESIN_FOOT'], buildSquatBed: ['PROP', 'RESIN_FOOT'] },
    configs: { PROP: () => propMod.PROP, RESIN_FOOT: () => footMod.RESIN_FOOT },
  },
);
propMod = await import(`${cap.rootUrl}web/prop/config.js`);
footMod = await import(`${cap.rootUrl}web/prop/resinfoot.js`);
const { PROP } = propMod;
const squat = cap.modules['prop/squat.js'];
const crestMod = cap.modules['prop/crest.js'];
const seating = cap.modules['fins/seating.js'];
const { surfaceZAt } = await import(`${cap.rootUrl}web/prop/surface.js`);
const util = await import(`${cap.rootUrl}tests/_util.js`);
const { siteOpts, sitePose, run: runScene } = await import(`${cap.rootUrl}tests/_scene.js`);

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

try {
  // --- direct: sweepSquat on synthetic low lines ---------------------------------------------
  const mk = (L, step, zOf, yOf = () => 0) => {
    const l = [];
    for (let x = -L / 2; x <= L / 2 + 1e-9; x += step) l.push([x, yOf(x), zOf(x)]);
    return l;
  };
  const gap = PROP.gap;
  const lines = [];
  for (const L of [4, 10, 24]) for (const h of [0.6, 0.9, 1.2, 1.49]) {
    lines.push([`flat-L${L}-h${h}`, mk(L, 1, () => gap + h)]);
    lines.push([`ramp-L${L}-h${h}`, mk(L, 1, (x) => gap + h + 0.02 * x)]);
    lines.push([`curve-L${L}-h${h}`, mk(L, 1, () => gap + h, (x) => 0.03 * x * x)]);
  }
  lines.push(['too-low', mk(6, 1, () => gap + 0.3)], ['tall', mk(6, 1, () => gap + 5)], ['one', [[0, 0, 1.2]]], ['repeat', [[0, 0, 1.2], [0, 0, 1.2], [3, 0, 1.2]]]);
  const variants = [
    ['plain', {}, null], ['round', { roundFeet: true }, null], ['pins', { tineDir: 'vertical' }, null],
    ['pins-inset', { tineDir: 'vertical' }, 'inset'], ['inset', {}, 'inset'], ['zeroinset', {}, 'zero'], ['shortinset', {}, 'short'],
    ['thick', { th: 3, tip: 1.2, squatBrimW: 4 }, null], ['brim', { squatBrimH: 0.8 }, null], ['tipH', { tipH: 3 }, null],
  ];
  const insetFor = (kind, n) => (kind === 'inset' ? Array.from({ length: n }, (_, i) => (i % 3 === 0 ? 0 : 0.2)) : kind === 'zero' ? new Array(n).fill(0) : kind === 'short' ? [0.1, 0.3] : null);
  const run = (tag, f) => { setScene(cap.sink, tag); crestMod.crestPart(null); f(); };
  lines.forEach(([name, line], i) => {
    variants.forEach(([vn, vals, ik], vi) => {
      if ((i + vi) % 3 && vn !== 'plain') return;
      run(`direct-sweepSquat-${vn}-${name}`, () => withVals(PROP, vals, () => squat.sweepSquat(line, 0, [], insetFor(ik, line.length))));
    });
  });
  // with the crest: a flat underside just above the low line, and a tilted one
  for (const [pn, part] of [['flat', Float64Array.from(block(-30, 30, -20, 20, 1.2 + gap - 0.0, 5))]]) {
    for (const iface of ['flat', 'all']) {
      for (const lh of [undefined, 0.2]) {
        setScene(cap.sink, `direct-sweepSquat-crest-${iface}-${pn}-lh${lh}`);
        crestMod.crestPart(part, lh);
        withVals(PROP, { iface }, () => squat.sweepSquat(mk(10, 1, () => 1.2 + gap), 0, [], null));
        withVals(PROP, { iface, ifaceLayers: 2 }, () => squat.sweepSquat(mk(10, 1, () => 1.2 + gap), 0, [], new Array(11).fill(0.1)));
        crestMod.crestPart(null);
      }
    }
  }

  // --- direct: buildSquatBed on the lowest surface of real parts -----------------------------
  const MODELS = ['ramp', 'wedge', 'lowledge', 'cone', 'sphere', 'torus', 'plate', 'arch', 'coin'];
  const POSES = [[0, 0, 0], [20, 0, 0], [0, 30, 0], [35, 0, 10]];
  for (const model of MODELS) {
    for (const deg of POSES) {
      const rot = sitePose(deg);
      setScene(cap.sink, `direct-setup-${model}-${deg}`);
      cap.sink.model = model;
      const t0 = util.loadModel(model);
      const r0 = util.analyze(t0, 45, rot);
      const tris0 = seating.seatedPartTris(t0, rot, r0.offset);
      let xs = [], ys = [];
      for (let i = 0; i < tris0.length; i += 3) { xs.push(tris0[i]); ys.push(tris0[i + 1]); }
      const lox = Math.min(...xs), hix = Math.max(...xs), loy = Math.min(...ys), hiy = Math.max(...ys);
      const cands = [];
      const along = (ax, ay, bx, by, n, minZ, maxZ) => {
        const l = [];
        for (let k = 0; k <= n; k++) {
          const x = ax + (bx - ax) * k / n, y = ay + (by - ay) * k / n;
          const z = surfaceZAt(tris0, x, y);
          if (z !== null) l.push([x, y, z]);
        }
        return l;
      };
      for (const f of [0.2, 0.4, 0.5, 0.6, 0.8]) {
        cands.push(along(lox + 0.5, loy + (hiy - loy) * f, hix - 0.5, loy + (hiy - loy) * f, 40));
        cands.push(along(lox + (hix - lox) * f, loy + 0.5, lox + (hix - lox) * f, hiy - 0.5, 40));
      }
      cands.push(along(lox + 0.5, loy + 0.5, hix - 0.5, hiy - 0.5, 50));
      cands.forEach((line, li) => {
        // only lines with at least a few low stations are interesting for the squat band
        const low = line.filter((p) => p[2] - gap >= PROP.minHeightSquat && p[2] - gap < PROP.minHeight).length;
        if (low < 2) return;
        for (const [vn, vals, claimedKind] of [['plain', {}, null], ['claimed', {}, 'half'], ['pins', { tineDir: 'vertical' }, null], ['crest', { iface: 'all' }, null], ['nospan', { minSpanSquat: 0.1, minStations: 2 }, null]]) {
          setScene(cap.sink, `direct-buildSquatBed-${model}-${deg}-l${li}-${vn}`);
          cap.sink.model = model;
          crestMod.crestPart(null);
          const tp = util.loadModel(model);
          const rs = util.analyze(tp, 45, rot);
          const tr = seating.seatedPartTris(tp, rot, rs.offset);
          if (vn === 'crest') crestMod.crestPart(tr, 0.2);
          const claimed = claimedKind ? line.map((_, k) => k < line.length / 2) : null;
          withVals(PROP, vals, () => squat.buildSquatBed(line, tr, tp, rot, rs.offset, [], claimed));
          crestMod.crestPart(null);
        }
      });
    }
  }

  // --- the golden scenes' builds ------------------------------------------------------------------
  const goldenDir = `${UPSTREAM_ROOT}/tests/golden`;
  const scenes = [...Deno.readDirSync(goldenDir)].filter((e) => e.name.endsWith('.json')).map((e) => e.name).sort()
    .map((n) => JSON.parse(Deno.readTextFileSync(`${goldenDir}/${n}`)).scene);
  for (const scene of scenes) {
    setScene(cap.sink, `build-${scene.name}`);
    cap.sink.model = scene.model;
    runScene(scene);
  }
  const extra = [['all', { iface: 'all' }], ['pins', { tineDir: 'vertical' }]];
  for (const scene of scenes.filter((s) => !s.draw && !s.set?.sway).filter((_, i) => i % 3 === 0)) {
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
    util.fins.applyTunables({ iface: false, tineDir: 'horizontal', pinDepth: 0.1 });
  }

  // --- assemble ------------------------------------------------------------------------------------
  const cpLog = new Map();
  for (const e of cap.sink.calls.crestPart.entries()) {
    if (!cpLog.has(e.scene)) cpLog.set(e.scene, []);
    cpLog.get(e.scene)[e.n] = e.args;
  }
  const stateOf = (scene, cp) => {
    const a = cp > 0 ? cpLog.get(scene)?.[cp - 1] : null;
    return a ? { part: a[0], layerHeight: a[1] ?? { $u: 1 } } : { part: null, layerHeight: { $u: 1 } };
  };
  const spread = (calls, n) => (calls.length <= n ? calls : Array.from({ length: n }, (_, i) => calls[Math.floor((i * calls.length) / n)]));
  const flat4 = (vs) => Float64Array.from(vs.flatMap((v) => [v[0], v[1], v[2], v[3] === 1 ? 1 : 0]));
  const isCfg = (a) => a !== null && typeof a === 'object' && '$config' in a;
  const finish = (fn, n) => {
    const all = cap.sink.calls[fn].entries();
    // buildSquatBed returns [] for most real calls: keep every kind, weighting to the walls it placed
    const entries = fn === 'buildSquatBed'
      ? [...spread(all.filter((e) => e.out.length > 0), Math.floor(n * 0.7)), ...spread(all.filter((e) => e.out.length === 0), Math.ceil(n * 0.3))]
      : spread(all, n);
    writeCases('squat', `${fn}.captured`, entries.map((e) => {
      const r = recs.get(`${fn}:${e.scene}#${e.n}`);
      if (!r) throw new Error(`no record for ${fn} ${e.scene}#${e.n}`);
      const cfgArg = e.args.at(-1);
      if (!isCfg(cfgArg)) throw new Error('config argument missing');
      const args = [...e.args.slice(0, -1), stateOf(e.scene, r.cp), cfgArg];
      const tris = flat4(r.rec);
      const res = fn === 'sweepSquat' ? { ret: e.out, tris: encode(tris) } : { placed: e.out, from: r.from, tris: encode(tris) };
      return { id: `${e.scene}#${e.n}`, args, out: res };
    }), { encoded: true });
  };
  finish('sweepSquat', 150);
  finish('buildSquatBed', 60);
  console.log('squat: calls', JSON.stringify({ sweepSquat: cap.sink.calls.sweepSquat.entries().length, buildSquatBed: cap.sink.calls.buildSquatBed.entries().length }));
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error('the upstream checkout changed during the run');
