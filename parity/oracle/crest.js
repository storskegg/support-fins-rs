// Oracle recorder for web/prop/crest.js -> parity/corpus/crest/<fn>.jsonl. Replayed in
// crates/sf-testkit/tests/parity_crest.rs with Tol::Exact (crest_kinds needs track C's
// surface port; the rest runs without it).
//
// JS keeps `part` / `layerH` in module state set by crestPart(); each case records it as a
// `state` argument {part: Float64Array | null, layerHeight: number | undefined}, which the
// replay turns into Crest::new(part, layerHeight, cfg). Every case ends with the
// {"$config": {"PROP": {...}}} snapshot. `P(o, z)`, which JS passes as a function, is fixed:
// P(o, z) = [2 + 0.5 * o, 3 - 0.25 * o, z].
// A vertex tag (`v[3] === 1`) is recorded as a boolean per vertex of the triangle list.
//   crest_on           args: [$config]                               out: boolean
//   crest_h            args: [state, $config]                        out: number
//   crest_kinds        args: [pts, state, $config]                   out: ['flat' | 'slope' | null, ...] | null
//   crest_cut          args: [top, ztip, floor, kind, th, tip, state, $config]
//                                                                    out: {cut, zt, wc, ring} | null
//   flat_cut           args: [top, state, $config]                   out: number
//   flat_crest_ring    args: [top, half, state, $config]             out: [[x, y, z] x 4]
//   emit_crest         args: [sections]                              out: {tris, tagged}
//   mark_interface     args: [tris, from]                            out: tagged
//   drop_collapsed     args: [tris, from]                            out: tris
//   split_interface    args: [tris, tagged]                          out: {body, iface}
import { assertPinned, WEB } from './lib/pin.js';
import { encode, writeCases } from './lib/encode.js';

assertPinned();
const crest = await import(`${WEB}prop/crest.js`);
const { PROP } = await import(`${WEB}prop/config.js`);

function rng(seed) {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13; s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5; s >>>= 0;
    return s / 4294967296;
  };
}
const R = rng(0xc4e57c4e);
const rnd = (lo, hi) => lo + R() * (hi - lo);
const flat = (tris) => Float64Array.from(tris.flatMap((v) => v.slice(0, 3))); // tagged vertices carry a 4th component
const P = (o, z) => [2 + 0.5 * o, 3 - 0.25 * o, z];
const cfg = () => ({ $config: { PROP: encode({ ...PROP }) } });
const prev = { ...PROP };
const setProp = (vals) => { Object.assign(PROP, prev, vals); };

function block(x0, x1, y0, y1, z0, z1) {
  const v = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
  const t = [];
  for (const [a, b, c] of [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]]) t.push(...v[a], ...v[b], ...v[c]);
  return t;
}
/** A slab tilted `deg` about Y, its underside through (0, 0, z0). */
function tilted(deg, z0) {
  const a = (deg * Math.PI) / 180, c = Math.cos(a), s = Math.sin(a);
  const t = block(-30, 30, -20, 20, 0, 4);
  for (let i = 0; i < t.length; i += 3) {
    const x = t[i], z = t[i + 2];
    t[i] = c * x + s * z; t[i + 2] = -s * x + c * z + z0;
  }
  return Float64Array.from(t);
}
const underside = (deg, z0) => (x) => z0 - Math.tan((deg * Math.PI) / 180) * x;

const states = [
  { part: null, layerHeight: undefined },
  { part: null, layerHeight: 0.2 },
  { part: null, layerHeight: 0.3 },
  { part: null, layerHeight: 0 },
  { part: null, layerHeight: NaN },
];
const propVariants = [
  {}, { iface: 'flat' }, { iface: 'all' }, { iface: 'flat', ifaceLayers: 3 }, { iface: 'all', ifaceLayers: 0 },
  { iface: false, ifaceLayers: 2 }, { iface: 'flat', ifaceFlatDeg: 25 }, { iface: 'all', ifaceFlatDeg: 0 },
];
const stateOf = (s) => { crest.crestPart(s.part, s.layerHeight); return s; };

const out = { on: [], h: [], kinds: [], cut: [], flatCut: [], ring: [], emit: [], mark: [], drop: [], split: [] };

propVariants.forEach((pv, vi) => {
  setProp(pv);
  out.on.push({ id: `v${vi}`, args: [cfg()], out: crest.crestOn() });
  states.forEach((s, si) => {
    const st = stateOf(s);
    out.h.push({ id: `v${vi}/s${si}`, args: [st, cfg()], out: crest.crestH() });
    for (const [ti, top] of [3, 10, 24.5, 100].entries()) {
      out.flatCut.push({ id: `v${vi}/s${si}/t${ti}`, args: [top, st, cfg()], out: crest.flatCut(top) });
      out.ring.push({ id: `v${vi}/s${si}/t${ti}`, args: [top, 0.6, st, cfg()], out: crest.flatCrestRing(P, top, 0.6) });
    }
    for (const kind of ['flat', 'slope', null]) {
      for (const [top, ztip, floor, th, tip] of [[20, 18.5, 0.6, 1, 0.6], [2, 1.9, 0.6, 1, 0.6], [1, 0.9, 0.5, 2.4, 1.5], [50, 40, 0, 6, 0.6], [20, 19, 0.6, 1, 1], [0.7, 0.6, 0.6, 1, 0.6], [20, 21, 0.6, 1, 0.6]]) {
        out.cut.push({
          id: `v${vi}/s${si}/${kind}/${top},${ztip},${floor},${th},${tip}`,
          args: [top, ztip, floor, kind, th, tip, st, cfg()],
          out: (() => { const c = crest.crestCut(top, ztip, floor, kind, th, tip); return c && { cut: c.cut, zt: c.zt, wc: c.wc, ring: c.ring(P) }; })(),
        });
      }
    }
  });
  // crest_kinds over real undersides (flat, nearly flat, steep, absent) with the part set
  const parts = [
    ['flat', Float64Array.from(block(-30, 30, -20, 20, 10, 14)), underside(0, 10)],
    ['t5', tilted(5, 12), underside(5, 12)], ['t9.9', tilted(9.9, 12), underside(9.9, 12)],
    ['t10.1', tilted(10.1, 12), underside(10.1, 12)], ['t35', tilted(35, 20), underside(35, 20)],
    ['t80', tilted(80, 30), underside(80, 30)],
  ];
  for (const [pn, part, under] of parts) {
    for (const lh of [undefined, 0.2]) {
      const st = stateOf({ part, layerHeight: lh });
      const lines = [
        [-10, -5, 0, 5, 10].map((x) => [x, 0, under(x)]),
        [-10, -5, 0, 5, 10].map((x) => [x, 0, under(x) + (x === 0 ? 3 : 0)]),   // one station off the surface
        [[-3, 0, under(-3)], [3, 0, under(3)]], [[0, 0, under(0)]],
        [[-40, 0, 12], [0, 0, under(0)], [40, 0, 12], [41, 0, 12]],            // beside the part
        [],
      ];
      lines.forEach((pts, li) => out.kinds.push({ id: `v${vi}/${pn}/lh${lh}/l${li}`, args: [pts, st, cfg()], out: crest.crestKinds(pts) }));
    }
  }
  crest.crestPart(null);
});
setProp({});

// emit / mark / drop / split: pure on triangle lists
const ring = (z, w) => [[w, 0, z], [w, 1, z], [-w, 1, z], [-w, 0, z]];
const tag = (arr, from) => { for (let i = from; i < arr.length; i++) arr[i][3] = 1; };
const tagged = (arr) => arr.map((v) => v[3] === 1);
const sectionSets = [
  [ring(0, 1), ring(1, 1), ring(2, 1)], [ring(0, 1), null, ring(2, 1), ring(3, 1)], [null, ring(0, 1), null],
  [ring(0, 1), ring(1, 1), null, ring(2, 1), ring(3, 1), ring(4, 1), null, null, ring(5, 1), ring(6, 1)], [], [null], [ring(0, 1)],
];
sectionSets.forEach((secs, i) => {
  const o = [];
  crest.emitCrest(secs, o);
  out.emit.push({ id: `${i}`, args: [secs], out: { tris: flat(o), tagged: tagged(o) } });
});
for (let i = 0; i < 25; i++) {
  const tris = [];
  const n = Math.floor(R() * 8);
  for (let t = 0; t < n; t++) {
    const a = [rnd(0, 3), rnd(0, 3), rnd(0, 3)], b = [rnd(0, 3), rnd(0, 3), rnd(0, 3)], c = [rnd(0, 3), rnd(0, 3), rnd(0, 3)];
    const kind = Math.floor(R() * 4);
    tris.push(a, kind === 1 ? a : b, kind === 2 ? b : c);          // repeated corners
    if (kind === 3) tris.push(a, b, a.map((x) => x));
  }
  const from = Math.floor(R() * (tris.length + 1));
  const copy = () => tris.map((v) => [...v]);
  out.mark.push({ id: `${i}`, args: [flat(tris), from], out: (() => { const t = copy(); crest.markInterface(t, from); return tagged(t); })() });
  out.drop.push({ id: `${i}`, args: [flat(tris), from], out: (() => { const t = copy(); crest.dropCollapsed(t, from); return flat(t); })() });
  const tg = tris.map(() => R() < 0.4);
  // tag whole triangles, as the engine does
  for (let t = 0; t + 2 < tg.length; t += 3) tg[t + 1] = tg[t + 2] = tg[t];
  out.split.push({ id: `${i}`, args: [flat(tris), tg], out: (() => { const t = copy(); t.forEach((v, k) => { if (tg[k]) v[3] = 1; }); const r = crest.splitInterface(t); return { body: flat(r.body), iface: flat(r.iface) }; })() });
}

const names = { on: 'crest_on', h: 'crest_h', kinds: 'crest_kinds', cut: 'crest_cut', flatCut: 'flat_cut', ring: 'flat_crest_ring', emit: 'emit_crest', mark: 'mark_interface', drop: 'drop_collapsed', split: 'split_interface' };
const isCfg = (a) => a !== null && typeof a === 'object' && '$config' in a;
for (const [k, fn] of Object.entries(names)) {
  writeCases('crest', fn, out[k].map((c) => ({ ...c, args: c.args.map((a) => (isCfg(a) ? a : encode(a))), out: encode(c.out) })), { encoded: true });
}
