// Oracle recorder for web/prop/tines.js (tineStepFor, emitTines) and web/prop/pins.js
// (emitPins) -> parity/corpus/tines/<fn>.captured.jsonl and parity/corpus/pins/emitPins.captured.jsonl.
// Replayed in crates/sf-testkit/tests/parity_tines.rs with Tol::Exact.
//
// Calls come from: direct-* (lines laid on the underside of real stress models, under several
// poses, with every positional rest argument, tinesPerWall, vertical pins, the interface crest
// tagging the comb) and build-* (the 54 golden scenes' own calls, plus builds with the crest on
// and vertical pins). Args:
//   tineStepFor  [density, $config{PROP}]                       out: number
//   emitTines    [line, tris ($seated), topo ($topo), rot, offset, out ([]), ...rest, $config{PROP}]
//   emitPins     the same
// `rest` is the positional (stepArg, minTop, tineH, body, grip) JS was called with (trailing
// arguments the caller omitted are absent; `undefined` is {"$u": 1}). The recorded out is
//   {n, tris, gripZ, seeds}: n the return value, tris a Float64Array of 4 numbers per vertex
//   (x, y, z, tag) pushed onto `out`, gripZ the `grip.z` after the call (null if no grip),
//   seeds the globalThis.__TINECAP contents the call pushed ([[x, y, z, biteX, biteY], ...], only
//   for direct cases, which control __TINECAP; null for build cases).
import { assertPinned, UPSTREAM_ROOT } from './lib/pin.js';
import { captureModules, setScene, upstreamStatus } from './lib/capture.js';
import { encode, writeCases } from './lib/encode.js';

assertPinned();
const statusBefore = upstreamStatus();

const recs = new Map(); // `${fn}:${scene}#${n}` -> {rec, grip}
const flat4 = (vs, untagged = false) => Float64Array.from(vs.flatMap((v) => [v[0], v[1], v[2], !untagged && v[3] === 1 ? 1 : 0]));
let propMod, pins;
// `out` is replaced by a proxy that forwards to the real array and records every push (the
// elements are shared, so a later markInterface tag shows in the record).
const recordOut = (fn) => (args, sink) => {
  const real = args[5];
  const rec = [];
  const proxy = new Proxy(real, {
    get(t, k) {
      if (k === 'push') return (...v) => { rec.push(...v); return t.push(...v); };
      const x = Reflect.get(t, k);
      return typeof x === 'function' ? x.bind(t) : x;
    },
  });
  const grip = args[10];
  recs.set(`${fn}:${sink.tag}#${sink.counts[fn] - 1}`, { rec, grip, seeds: null });
  const rest = [...args];
  rest[5] = proxy;
  // the recorded `out` argument is the empty array it was at the call
  const shown = [...args];
  shown[5] = [];
  Object.defineProperty(shown, '__call', { value: rest, enumerable: false });
  args.splice(0, args.length, ...rest); // the call runs with the proxy
  return shown;
};

const cap = await captureModules(
  [
    { rel: 'overhangs.js', fns: ['buildTopology', 'analyze'] },
    { rel: 'fins/seating.js', fns: ['seatedPartTris'] },
    { rel: 'prop/tines.js', fns: ['tineStepFor', 'emitTines'] },
    { rel: 'prop/pins.js', fns: ['emitPins'] },
  ],
  {
    cap: 1e9,
    refs: true,
    copy: ['tests/_util.js', 'tests/_scene.js', 'prototype/stress/models/'],
    args: {
      buildTopology: ([g]) => [g.getAttribute('position').array],
      analyze: ([, th, rot], sink) => [sink.model, th, rot],
      emitTines: recordOut('emitTines'),
      emitPins: recordOut('emitPins'),
    },
    config: { tineStepFor: ['PROP'], emitTines: ['PROP'], emitPins: ['PROP'] },
    configs: { PROP: () => propMod.PROP },
  },
);
propMod = await import(`${cap.rootUrl}web/prop/config.js`);
const { PROP } = propMod;
const tinesMod = cap.modules['prop/tines.js'];
const { emitTines, tineStepFor } = tinesMod;
const { surfaceZAt } = await import(`${cap.rootUrl}web/prop/surface.js`);
const seating = cap.modules['fins/seating.js'];
const util = await import(`${cap.rootUrl}tests/_util.js`);
const { siteOpts, sitePose, run: runScene } = await import(`${cap.rootUrl}tests/_scene.js`);

function withVals(obj, vals, fn) {
  const was = {};
  for (const k of Object.keys(vals)) was[k] = obj[k];
  Object.assign(obj, vals);
  try { return fn(); } finally { Object.assign(obj, was); }
}

try {
  // --- tineStepFor ----------------------------------------------------------------------------
  setScene(cap.sink, 'direct-step');
  for (const d of [undefined, null, -1, 0, 0.25, 0.5, 0.99, 1, 2, NaN]) tineStepFor(d);
  withVals(PROP, { tineStep: 1, tineStepSparse: 8 }, () => { for (const d of [undefined, 0, 0.5, 1]) tineStepFor(d); });

  // --- direct: lines laid on real undersides ------------------------------------------------------
  const MODELS = ['lowledge', 'ramp', 'wedge', 'sphere', 'cone', 'torus', 'cube', 'arch', 'coin', 'plate'];
  const POSES = [[0, 0, 0], [35, 0, 0], [0, 25, 0], [20, 30, 10]];
  const rotOf = (deg) => sitePose(deg);
  let cases = 0;
  for (const model of MODELS) {
    for (const deg of POSES) {
      const rot = rotOf(deg);
      setScene(cap.sink, `direct-setup-${model}-${deg}`);
      cap.sink.model = model;
      const topo = util.loadModel(model);
      const res = util.analyze(topo, 45, rot);
      const tris = seating.seatedPartTris(topo, rot, res.offset);
      // candidate lines: along X and Y through a few points, and a diagonal, on the part's lowest surface
      let xs = [], ys = [];
      for (let i = 0; i < tris.length; i += 3) { xs.push(tris[i]); ys.push(tris[i + 1]); }
      const lox = Math.min(...xs), hix = Math.max(...xs), loy = Math.min(...ys), hiy = Math.max(...ys);
      const lines = [];
      for (const f of [0.3, 0.5, 0.7]) {
        const along = (ax, ay, bx, by, n) => {
          const l = [];
          for (let k = 0; k <= n; k++) {
            const x = ax + (bx - ax) * k / n, y = ay + (by - ay) * k / n;
            const z = surfaceZAt(tris, x, y);
            if (z !== null && z > 0.8) l.push([x, y, z]);
          }
          return l;
        };
        lines.push(along(lox + 1, loy + (hiy - loy) * f, hix - 1, loy + (hiy - loy) * f, 24));
        lines.push(along(lox + (hix - lox) * f, loy + 1, lox + (hix - lox) * f, hiy - 1, 24));
      }
      lines.push(along0(lox + 1, loy + 1, hix - 1, hiy - 1, 30));
      function along0(ax, ay, bx, by, n) {
        const l = [];
        for (let k = 0; k <= n; k++) {
          const x = ax + (bx - ax) * k / n, y = ay + (by - ay) * k / n;
          const z = surfaceZAt(tris, x, y);
          if (z !== null && z > 0.8) l.push([x, y, z]);
        }
        return l;
      }
      lines.forEach((line, li) => {
        if (line.length < 2) return;
        const variants = [
          ['plain', {}, []],
          ['rev', {}, [], true],
          ['step5', {}, [5]],
          ['step999', {}, [999]],
          ['minTop', {}, [undefined, 3]],
          ['h06', {}, [undefined, undefined, 0.6]],
          ['h03', {}, [0.5, undefined, 0.3]],
          ['body', {}, [undefined, undefined, undefined, [1, Math.max(1, line.length - 2)]]],
          ['grip', {}, [undefined, undefined, undefined, null, { z: Infinity }]],
          ['count4', { tinesPerWall: 4 }, []],
          ['pins', { tineDir: 'vertical' }, [undefined, undefined, undefined, null, { z: Infinity }]],
          ['crest', { iface: 'all' }, []],
          ['wide', { tineW: 1.2, tineOverlap: 0.6, tineReach: 0.9 }, []],
        ];
        variants.forEach(([vn, vals, rest, rev], vi) => {
          if ((li + vi + deg[0]) % 3 && vn !== 'plain' && vn !== 'grip') return;
          const l = rev ? [...line].reverse() : line;
          setScene(cap.sink, `direct-${model}-${deg}-l${li}-${vn}`);
          cap.sink.model = model;
          // each scene needs its own topology reference (buildTopology registers per call)
          const tp = util.loadModel(model);
          const rs = util.analyze(tp, 45, rot);
          const tr = seating.seatedPartTris(tp, rot, rs.offset);
          const seeds = [];
          globalThis.__TINECAP = seeds;
          const out = [];
          const rcopy = rest.map((x) => (x && typeof x === 'object' && !Array.isArray(x) ? { ...x } : x));
          withVals(PROP, vals, () => emitTines(l, tr, tp, rot, rs.offset, out, ...rcopy));
          globalThis.__TINECAP = undefined;
          const e = recs.get(`emitTines:${cap.sink.tag}#0`);
          e.seeds = seeds.map((s) => [s.x, s.y, s.z, s.biteX, s.biteY]);
          cases++;
        });
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
  const picked = ['cube-x40-auto', 'lbracket-x35-auto', 'stress-ramp-x40', 'stress-wedge-up', 'stress-tshape-x40', 'stress-torus-x40', 'stress-sphere-x40'];
  const extra = [['all', { iface: 'all' }], ['pins', { tineDir: 'vertical' }], ['petg', { propGap: 0.3, padH: 0.3 }]];
  for (const scene of scenes.filter((s) => !s.draw && !s.set?.sway && picked.some((p) => s.name.startsWith(p)))) {
    cap.sink.model = scene.model; // the topology's reference is the model id current when it is built
    const rot = sitePose(scene.rot);
    const topo = util.loadModel(scene.model);
    const res = util.analyze(topo, 45, rot);
    for (const [label, tun] of extra) {
      setScene(cap.sink, `build-${scene.name}-${label}`);
      cap.sink.model = scene.model;
      const opts = siteOpts(scene.set);
      opts.layerHeight = label === 'petg' ? 0.3 : 0.2;
      opts.tunables = { ...opts.tunables, ...tun };
      util.fins.buildFins(topo, res, rot, opts);
    }
    util.fins.applyTunables({ iface: false, tineDir: 'horizontal', pinDepth: 0.1, propGap: 0.2, padH: 0.5 });
  }

  // --- write ---------------------------------------------------------------------------------------
  const spread = (calls, n) => (calls.length <= n ? calls : Array.from({ length: n }, (_, i) => calls[Math.floor((i * calls.length) / n)]));
  const finishOut = (fn, mod, n) => {
    const entries = spread(cap.sink.calls[fn].entries(), n);
    writeCases(mod, `${fn}.captured`, entries.map((e) => {
      const r = recs.get(`${fn}:${e.scene}#${e.n}`);
      if (!r) throw new Error(`no record for ${fn} ${e.scene}#${e.n}`);
      // `grip` is the 11th JS argument (index 10 counting the proxy-less call); read its z now
      const gz = r.grip && typeof r.grip === 'object' ? r.grip.z : null;
      // emitPins never tags; emitTines' markInterface tags its vertices afterwards
      return {
        id: `${e.scene}#${e.n}`, args: e.args,
        out: { n: e.out, tris: encode(flat4(r.rec, fn === 'emitPins')), gripZ: encode(gz), seeds: encode(r.seeds) },
      };
    }), { encoded: true });
  };
  const stepEntries = spread(cap.sink.calls.tineStepFor.entries(), 80);
  writeCases('tines', 'tineStepFor.captured', stepEntries.map((e) => ({ id: `${e.scene}#${e.n}`, args: e.args, out: e.out })), { encoded: true });
  finishOut('emitTines', 'tines', 150);
  finishOut('emitPins', 'pins', 70);
  console.log('tines: calls', JSON.stringify({ emitTines: cap.sink.calls.emitTines.entries().length, emitPins: cap.sink.calls.emitPins.entries().length, direct: cases }));
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error('the upstream checkout changed during the run');
