// Oracle recorder for web/cutout.js (holesFor, honeycombHoles, cutWall) and
// web/prop/resinfoot.js (windowedFlange) -> parity/corpus/{cutout,resinfoot}/<fn>.captured.jsonl.
// Replayed in crates/sf-testkit/tests/parity_cutout.rs and parity_resinfoot.rs with Tol::Exact.
//
// Every function is wrapped (capture-by-rename), and the calls come from three sources, so one
// replay covers them all:
//   direct-*  : holesFor / honeycombHoles over grids of arguments, and cutWall / windowedFlange
//               through the real prop/sweep.js on synthetic lines (straight, sloped, curved, wavy;
//               every pattern; a few CUT / PROP / RESIN_FOOT variants);
//   build-*   : cutWall / windowedFlange as real golden-scene builds call them, with the
//               cutout pattern (or vertical pins) switched on. The 54 golden scenes use neither.
//
// Args (then the trailing {"$config": {...}} snapshot of the config objects each reads):
//   holesFor         [kind, w, H]                        out: [{a, z0, z1}, ...]
//   honeycombHoles   [s, zLo, zHi, sA, sB]               out: [[[s, z], ...], ...]
//   cutWall          [st, full, [], wall] + {CUT}        out: {ret, tris}
//                    (`wall` = {th, tip, minStations}: PROP from prop/sweep.js, or the wedge blade's own)
//   windowedFlange   [feet, []]      + {RESIN_FOOT}      out: {ret, tris}
// `tris` is a flat Float64Array of the vertices the call pushed onto its `out` array (a cutWall
// that returns false pushes nothing), recorded at call time; the `out` argument itself is
// recorded as [].
import { assertPinned, UPSTREAM_ROOT } from './lib/pin.js';
import { captureModules, flush, setScene, upstreamStatus } from './lib/capture.js';
import { encode } from './lib/encode.js';

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
const R = rng(0xc07a0b1e);
const rnd = (lo, hi) => lo + R() * (hi - lo);

// What an out-array call pushed, recorded at call time: the hook swaps the real `out` for a
// recorder that forwards to it (these functions only ever `push` onto it).
const recs = new Map();
const recordOut = (name, outIndex, keep, after = []) => (args, sink) => {
  const real = args[outIndex];
  const rec = [];
  args[outIndex] = { push: (...v) => { rec.push(...v); return real.push(...v); } };
  recs.set(`${name}:${sink.tag}#${sink.counts[name] - 1}`, rec);
  return [...keep.map((i) => args[i]), [], ...after.map((i) => args[i])];
};

const cap = await captureModules(
  [
    { rel: 'cutout.js', fns: ['holesFor', 'honeycombHoles', 'cutWall'] },
    { rel: 'prop/resinfoot.js', fns: ['windowedFlange'] },
  ],
  {
    cap: 60,
    copy: ['tests/_util.js', 'tests/_scene.js', 'prototype/stress/models/'],
    args: {
      cutWall: recordOut('cutWall', 2, [0, 1], [3]),
      windowedFlange: recordOut('windowedFlange', 1, [0]),
    },
    config: { holesFor: ['CUT'], honeycombHoles: ['CUT'], cutWall: ['CUT'], windowedFlange: ['RESIN_FOOT'] },
    configs: {
      PROP: () => propMod.PROP,
      CUT: () => cutMod.CUT,
      RESIN_FOOT: () => footMod.RESIN_FOOT,
    },
  },
);
const cutMod = cap.modules['cutout.js'];
const footMod = cap.modules['prop/resinfoot.js'];
const propMod = await import(`${cap.rootUrl}web/prop/config.js`);
const { sweep, sweepBetween } = await import(`${cap.rootUrl}web/prop/sweep.js`);
const { holesFor, honeycombHoles, CUT } = cutMod;
const { PROP } = propMod;
const { RESIN_FOOT } = footMod;

/** Sets fields on a config object for the duration of `fn`. */
function withVals(obj, vals, fn) {
  const was = {};
  for (const k of Object.keys(vals)) was[k] = obj[k];
  Object.assign(obj, vals);
  try { return fn(); } finally { Object.assign(obj, was); }
}

try {
  // --- holesFor ------------------------------------------------------------------------------
  setScene(cap.sink, 'direct-holes');
  const KINDS = ['diamond', 'triangle', 'arch', 'lattice', 'honeycomb', 'none', 'bogus'];
  const WS = [0.5, 1, 2.4, 3, 4, 6, 8, 12, 20, 40];
  const HS = [0.5, 2, 3, 5, 8, 12, 20, 40, 100];
  for (const k of KINDS) for (const w of WS) for (const h of HS) holesFor(k, w, h);
  for (const vals of [{ slope: 1.0, web: 2.5 }, { maxSlope: 2.0, minHole: 1.5 }, { minHalf: 0.4, slope: 2.2 }]) {
    withVals(CUT, vals, () => {
      for (const k of ['diamond', 'triangle', 'arch']) for (const w of WS) for (const h of HS) holesFor(k, w, h);
    });
  }

  // --- honeycombHoles ------------------------------------------------------------------------
  setScene(cap.sink, 'direct-honeycomb');
  const stations = (L, step) => { const s = []; for (let x = 0; x <= L + 1e-9; x += step) s.push(x); return s; };
  const zones = [
    (s) => [s.map(() => 1.0), s.map(() => 30)],
    (s) => [s.map(() => 2.0), s.map((x) => 40 - 0.5 * x)],
    (s) => [s.map((x) => 1 + 0.3 * x), s.map((x) => 30 - 0.2 * x)],
    (s) => [s.map(() => 0.6), s.map((x) => 12 + 10 * Math.sin(x / 6))],
    (s) => [s.map((x) => Math.abs(x - 15) * 0.4), s.map((x) => 20 + Math.abs(x - 15) * 0.8)],
    (s) => [s.map(() => 5), s.map(() => 6)],
  ];
  for (const L of [10, 18, 30, 60]) {
    for (const z of zones) {
      const s = stations(L, 1);
      const [lo, hi] = z(s);
      honeycombHoles(s, lo, hi, 2, L - 2);
      honeycombHoles(s, lo, hi, 0, L);
    }
  }
  for (const vals of [{ hexPitch: 4, hexStrut: 0.8 }, { hexSlope: 2.0 }, { hexPitch: 9, hexStrut: 2 }, { roofMin: 1.4 }]) {
    withVals(CUT, vals, () => {
      const s = stations(30, 1);
      for (const z of zones) { const [lo, hi] = z(s); honeycombHoles(s, lo, hi, 2, 28); }
    });
  }

  // --- cutWall / windowedFlange through sweep / sweepBetween on synthetic lines ---------------
  const lines = [];
  const mk = (L, step, zOf, yOf = () => 0) => {
    const l = [];
    for (let x = -L / 2; x <= L / 2 + 1e-9; x += step) l.push([x, yOf(x), zOf(x)]);
    return l;
  };
  for (const L of [14, 30]) {
    for (const Z of [10, 24, 40]) {
      lines.push([`flat-L${L}-Z${Z}`, mk(L, 1, () => Z)]);
      lines.push([`slope-L${L}-Z${Z}`, mk(L, 1, (x) => Z - 0.5 * x)]);
      lines.push([`wavy-L${L}-Z${Z}`, mk(L, 1, (x) => Z + 3 * Math.sin(x / 4))]);
      lines.push([`curve-L${L}-Z${Z}`, mk(L, 1, () => Z, (x) => 0.02 * x * x)]);
    }
  }
  const PATTERNS = ['diamond', 'triangle', 'arch', 'lattice', 'honeycomb'];
  let pick = 0;
  // an interleaved selection, so a thinned corpus still holds every pattern and line shape
  for (let i = 0; i < lines.length; i++) {
    for (let p = 0; p < PATTERNS.length; p++) {
      if ((i + p) % 3 !== 0) continue;
      const [name, line] = lines[i];
      setScene(cap.sink, `direct-sweep-${PATTERNS[p]}-${name}`);
      CUT.pattern = PATTERNS[p];
      sweep(line, 0, []);
      pick++;
    }
  }
  // a floor under the line: sweepBetween (tilted lifted bottoms, welded or not)
  for (const [name, line] of lines.filter((_, i) => i % 5 === 0)) {
    const floor = line.map(([x, y]) => [x, y, 2 + 0.05 * x, 1.5 + 0.05 * x, 2.5 + 0.05 * x]);
    for (const pat of ['diamond', 'lattice', 'honeycomb']) {
      for (const footGap of [0.2, 0]) {
        setScene(cap.sink, `direct-between-${pat}-${name}-fg${footGap}`);
        CUT.pattern = pat;
        withVals(PROP, { footGap }, () => sweepBetween(line, floor, []));
      }
    }
  }
  // CUT / PROP variants
  const base = lines.find(([n]) => n === 'slope-L30-Z40')[1];
  const variants = [
    { post: 4, pitch: 6 }, { pitch: 12, web: 2.4 }, { rail: 2.5 }, { minSaved: 0.4 }, { minSaved: 0 },
    { latPitch: 9, latStrut: 2 }, { hexPitch: 9, hexStrut: 2 }, { eps: 0.1 }, { roofMin: 1.5, minArea: 6 },
  ];
  variants.forEach((vals, i) => {
    for (const pat of ['diamond', 'lattice', 'honeycomb']) {
      setScene(cap.sink, `direct-cut-variant${i}-${pat}`);
      withVals(CUT, { pattern: pat, ...vals }, () => sweep(base, 0, []));
    }
  });
  for (const [i, vals] of [{ th: 3 }, { th: 6, tip: 1.5 }, { minStations: 100 }, { tipH: 4 }].entries()) {
    setScene(cap.sink, `direct-prop-variant${i}`);
    withVals(PROP, vals, () => withVals(CUT, { pattern: 'lattice' }, () => sweep(base, 0, [])));
  }
  // wedge blades (fins/wedges.js emitBlade): flat-topped stations, no sh / holeTop / tipHalf,
  // and a wall of its own numbers
  for (const [name, line] of lines.filter((_, i) => i % 4 === 1)) {
    for (const pat of ['diamond', 'lattice', 'honeycomb']) {
      setScene(cap.sink, `direct-blade-${pat}-${name}`);
      const st = [], full = [];
      for (let i = 0; i < line.length; i++) {
        const a = line[Math.max(0, i - 1)], b = line[Math.min(line.length - 1, i + 1)];
        let rx = b[0] - a[0], ry = b[1] - a[1];
        const rn = Math.hypot(rx, ry);
        rx /= rn; ry /= rn;
        const sx = ry, sy = -rx, p = line[i], z = p[2];
        const P = (o, zz) => [p[0] + sx * o, p[1] + sy * o, zz];
        full.push([P(1.2, 0), P(1.2, z), P(-1.2, z), P(-1.2, 0)]);
        st.push({ p, sx, sy, top: z, ztip: z - 0.05, bot: 0, botTip: 0.6, taperBot: false });
      }
      CUT.pattern = pat;
      cutMod.cutWall(st, full, [], { th: 1.2, tip: 1.2, minStations: 3 });
    }
  }
  CUT.pattern = 'none';
  setScene(cap.sink, 'direct-none');
  sweep(base, 0, []);

  // resin pins: the windowed flange (the strip + pads), through sweep with vertical tines
  for (const [name, line] of lines.filter((_, i) => i % 3 === 0)) {
    setScene(cap.sink, `direct-feet-${name}`);
    withVals(PROP, { tineDir: 'vertical' }, () => sweep(line, 0, []));
  }
  for (const [i, vals] of [{ pad: 3, window: 4 }, { strip: 0.8, bevel: 0.5 }, { pad: 1, window: 1 }, { pad: 20 }].entries()) {
    setScene(cap.sink, `direct-feet-variant${i}`);
    withVals(RESIN_FOOT, vals, () => withVals(PROP, { tineDir: 'vertical' }, () => sweep(base, 0, [])));
  }
  // flanges at and around the length threshold pad + window + pad = 5
  for (const L of [1, 3, 4.9, 5, 5.1, 7.4, 8, 20]) {
    setScene(cap.sink, `direct-feet-L${L}`);
    withVals(PROP, { tineDir: 'vertical' }, () => sweep(mk(L, L / 6, () => 30), 0, []));
  }

  // --- the golden scenes' builds, with the cutout patterns and vertical pins switched on -------
  const { siteOpts, sitePose } = await import(`${cap.rootUrl}tests/_scene.js`);
  const util = await import(`${cap.rootUrl}tests/_util.js`);
  const goldenDir = `${UPSTREAM_ROOT}/tests/golden`;
  const scenes = [...Deno.readDirSync(goldenDir)].filter((e) => e.name.endsWith('.json')).map((e) => e.name).sort()
    .map((n) => JSON.parse(Deno.readTextFileSync(`${goldenDir}/${n}`)).scene)
    .filter((s) => !s.draw && !s.set?.sway);
  const picked = ['cube-x40-auto', 'lbracket-x35-auto', 'stress-ramp-x40', 'stress-wedge-up', 'stress-tshape-x40',
    'stress-portal-x40', 'stress-plate-x40', 'stress-staircase-x40'];
  const builds = [
    ['diamond', { cutout: 'diamond' }], ['triangle', { cutout: 'triangle' }], ['arch', { cutout: 'arch' }],
    ['lattice', { cutout: 'lattice' }], ['honeycomb', { cutout: 'honeycomb' }], ['vertical', { tineDir: 'vertical' }],
  ];
  for (const scene of scenes.filter((s) => picked.some((p) => s.name.startsWith(p)))) {
    const rot = sitePose(scene.rot);
    const topo = util.loadModel(scene.model);
    const res = util.analyze(topo, 45, rot);
    for (const [label, extra] of builds) {
      setScene(cap.sink, `build-${scene.name}-${label}`);
      const opts = siteOpts(scene.set);
      opts.tunables = { ...opts.tunables, ...extra };
      util.fins.buildFins(topo, res, rot, opts);
    }
    // module state back to the defaults for the next scene
    util.fins.applyTunables({ cutout: 'none', tineDir: 'horizontal', pinDepth: 0.1 });
  }

  // --- patch cutWall / windowedFlange outs with what they pushed, then write ---------------------
  const flat = (tris) => Float64Array.from(tris.flat());
  for (const name of ['cutWall', 'windowedFlange']) {
    for (const e of cap.sink.calls[name]) {
      const rec = recs.get(`${name}:${e.scene}#${e.n}`);
      if (!rec) throw new Error(`no recorded out for ${name} ${e.scene}#${e.n}`);
      e.out = { ret: e.out, tris: encode(flat(rec)) };
    }
  }
  const counts = Object.fromEntries(Object.entries(cap.sink.calls).map(([k, v]) => [k, v.length]));
  // Evenly spaced picks across the whole run (direct cases first, golden-scene builds last), so
  // the thinned corpus still holds every kind; `thin()` would keep only the first ones.
  const spread = (calls, n) => (calls.length <= n ? calls : Array.from({ length: n }, (_, i) => calls[Math.floor((i * calls.length) / n)]));
  const out = (calls, n) => ({ cap: Infinity, calls });
  flush('cutout', out({ holesFor: spread(cap.sink.calls.holesFor, 500) }));
  flush('cutout', out({ honeycombHoles: spread(cap.sink.calls.honeycombHoles, 80) }));
  flush('cutout', out({ cutWall: spread(cap.sink.calls.cutWall, 60) }));
  flush('resinfoot', out({ windowedFlange: spread(cap.sink.calls.windowedFlange, 40) }));
  console.log('cutout: captured', JSON.stringify(counts));
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error('the upstream checkout changed during the run');
