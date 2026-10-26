// Oracle recorder for the upstream test helpers (tests/_util.js, _bore.js, _circular.js)
// -> parity/corpus/testkit/<fn>.jsonl. Replayed in crates/sf-testkit/tests/parity_testkit.rs
// with Tol::Exact.
//
// Receivers: none (all plain functions). Recorded functions:
//   block, tiltedBlockSoup, holedPlateSoup   args: the numbers; out: Float32Array (the soup as stored).
//                                            The last two record `topo.pos` of tiltedBlockTopo / holedPlateTopo.
//   circularBore, recessedBody               args: [options object] (only the keys the test passes); out: Float64Array
//   rotX, rotY                               args: [deg]; out: array of 9 numbers
//   ptTriDist2                               args: [p, a, b, c]; out: number
//   isClosed, isOriented                     args: [array of [x,y,z]]; out: boolean
//   bbox                                     args: [array of [x,y,z]]; out: {lo, hi}
//   tineBoxes                                args: [array of [x,y,z]]; out: {tines, steps} where each step's
//                                            `tine` (an object reference in JS) is recorded as its index in `tines`.
import { assertPinned, UPSTREAM_ROOT } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();
const util = await import(`file://${UPSTREAM_ROOT}/tests/_util.js`);
const { circularBore } = await import(`file://${UPSTREAM_ROOT}/tests/_bore.js`);
const { recessedBody } = await import(`file://${UPSTREAM_ROOT}/tests/_circular.js`);
const { block, tiltedBlockTopo, holedPlateTopo, rotX, rotY, ptTriDist2, isClosed, isOriented, bbox, tineBoxes } = util;

const id = (...a) => a.map(String).join(',');
const verts = (soup) => {
  const v = [];
  for (let i = 0; i < soup.length; i += 3) v.push([soup[i], soup[i + 1], soup[i + 2]]);
  return v;
};
const f32 = (a) => {
  if (!(a instanceof Float32Array) || a.constructor !== Float32Array) throw new Error('expected a plain Float32Array');
  return a;
};

// --- block / tilted / holed ---------------------------------------------------------------
const BLOCKS = [
  [0, 1, 0, 1, 0, 1], [-1, 0.5, 0, 1, 0, 1], [-1, 1, -1, 1, 5, 35], [-1, 11, -1, 11, -1, 11],
  [-1, 5, -1, 5, 5, 15], [-10, 0, -8, 8, 1.2, 6], [-10, 0.01, -5, 5, 1.8, 6], [-10, 10, -10, 10, 0, 20],
  [-10, 10, -5, 5, 0.6, 4], [-10, 10, -5, 5, 8.1, 14], [-12, 12, -6, 6, 4.2, 6], [-15, 15, -10, 10, 0, 24],
  [19, 21, -1, 1, 0, 2], [30, 60, -10, 10, 10, 15], [0.1, 0.7, 0.3, 0.9, 0.2, 1 / 3], [-1e-9, 1e9, 0, 1e-3, -0, 7],
];
const TILTS = [
  [-40, 40, -45, 45, -6, 6, 55], [-20, 20, -20, 20, -20, 20, 35], [-20, 20, -30, 30, -6, 6, 45],
  [-15, 15, -20, 20, -6, 6, 50], [-20, 20, -20, 20, -3, 3, 55], [0, 60, 0, 40, 0, 12, 60],
  [-20, 20, -15, 15, 0, 30, 45], [-20, 20, -30, 30, -20, 20, 45], [-20, 20, -30, 30, -20, 20, 46],
  [-15, 15, -20, 20, 0, 30, 40], [-45, 45, -45, 45, -3, 3, 30], [-15, 15, -25, 25, 0, 40, 40],
  [-25, 25, -6, 6, 0, 190, 10], [-25, 25, -6, 6, 0, 190, 25], [-25, 25, -6, 6, 0, 190, 70],
  [-20, 20, -15, 15, 0, 30, 30], [-20, 20, -15, 15, 0, 30, 60], [-15, 15, -20, 20, 0, 30, 45],
  [0, 1, 0, 1, 0, 1, 0], [0, 1, 0, 1, 0, 1, 90], [0, 1, 0, 1, 0, 1, -37.5], [-3, 7, -2, 9, -1, 4, 12.3456],
  [0, 1, 0, 1, 0, 1, 180], [0, 1, 0, 1, 0, 1, 360],
];
const HOLES = [
  [20, 20, 30, 5, 5], [40, 30, 4, 9, 9], [20, 20, 30, 2, 2], [20, 20, 30, 8, 8], [20, 20, 30, 10, 10],
  [30, 20, 5, 4, 3], [12.5, 7.25, 1.5, 3.3, 1.1],
];
writeCases('testkit', 'block', BLOCKS.map((a) => ({ id: id(...a), args: a, out: f32(block(...a)) })));
writeCases('testkit', 'tiltedBlockSoup', TILTS.map((a) => ({
  id: id(...a), args: a, out: f32(tiltedBlockTopo(...a).pos),
})));
writeCases('testkit', 'holedPlateSoup', HOLES.map((a) => ({
  id: id(...a), args: a, out: f32(holedPlateTopo(...a).pos),
})));

// --- circularBore / recessedBody ----------------------------------------------------------
const BORES = [
  {}, { diameter: 4 }, { diameter: 3 }, { diameter: 5.8 }, { diameter: 10 },
  { diameter: 4, segments: 64, axialSegments: 2 }, { diameter: 4, segments: 96, axialSegments: 1 },
  { diameter: 4, segments: 64, axialSegments: 1 }, { diameter: 4, segments: 96, axialSegments: 2 },
  { length: 17.3, half: 6.5, segments: 12 }, { segments: 7 },
];
writeCases('testkit', 'circularBore', BORES.map((o) => ({ id: JSON.stringify(o), args: [o], out: circularBore(o) })));
const BODIES = [
  {}, { translate: [110, 0, 0] }, { segments: 8 }, { segments: 7, yaw: 13 }, { yaw: 45, translate: [1.5, -2.25, 3] },
  { height: 25, opening: 20, bore: 10, depth: 6, outer: 30 }, { segments: 64, yaw: 90 },
  { segments: 360, height: 60, yaw: -17.5, translate: [0.1, 0.2, 0.3] },
];
writeCases('testkit', 'recessedBody', BODIES.map((o) => ({ id: JSON.stringify(o), args: [o], out: recessedBody(o) })));

// --- rotX / rotY --------------------------------------------------------------------------
const degs = [];
for (let d = -360; d <= 360; d++) degs.push(d);
writeCases('testkit', 'rotX', degs.map((d) => ({ id: `deg${d}`, args: [d], out: rotX(d) })));
writeCases('testkit', 'rotY', degs.map((d) => ({ id: `deg${d}`, args: [d], out: rotY(d) })));

// --- ptTriDist2: 5000 fixed-seed cases ----------------------------------------------------
let seed = 0x5eed1234;
const rnd = () => { // mulberry32
  seed = (seed + 0x6D2B79F5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pt = (s) => [(rnd() - 0.5) * s, (rnd() - 0.5) * s, (rnd() - 0.5) * s];
const dist = [];
for (let i = 0; i < 5000; i++) {
  const s = [1, 10, 100][i % 3];
  let a = pt(s), b = pt(s), c = pt(s), p = pt(s * 1.5);
  const mode = i % 10;
  if (mode === 7) p = [...a];                          // exactly a vertex
  else if (mode === 8) p = a.map((v, k) => v + 0.5 * (b[k] - a[k]));   // on an edge
  else if (mode === 9) { c = [...b]; }                 // degenerate triangle
  else if (mode === 6) { b = a.map((v, k) => v + 2 * (c[k] - v) * 0.5); p = a.map((v, k) => (v + c[k]) / 2); } // collinear
  dist.push({ id: `rand${String(i).padStart(4, '0')}`, args: [p, a, b, c], out: ptTriDist2(p, a, b, c) });
}
writeCases('testkit', 'ptTriDist2', dist);

// --- isClosed / isOriented / bbox / tineBoxes ----------------------------------------------
const soups = {
  block: block(-1, 2, 0, 3, 1, 5),
  unit: block(0, 1, 0, 1, 0, 1),
  tilted45: tiltedBlockTopo(-20, 20, -30, 30, -6, 6, 45).pos,
  tilted55: tiltedBlockTopo(-40, 40, -45, 45, -6, 6, 55).pos,
  holed: holedPlateTopo(20, 20, 30, 5, 5).pos,
  holedSmall: holedPlateTopo(40, 30, 4, 9, 9).pos,
  bore: circularBore({ segments: 12 }),
  bore96: circularBore(),
  recessed: recessedBody({ segments: 12 }),
  recessedYaw: recessedBody({ segments: 7, yaw: 13 }),
  empty: new Float32Array(0),
};
const shapes = [];
for (const [name, soup] of Object.entries(soups)) {
  const v = verts(soup);
  shapes.push([name, v]);
  if (v.length >= 3) {
    shapes.push([`${name}+flipFirst`, v.map((p, i) => (i === 1 ? v[2] : i === 2 ? v[1] : p))]);
    shapes.push([`${name}+flipAll`, v.map((p, i) => v[i - (i % 3) + [0, 2, 1][i % 3]])]);
    shapes.push([`${name}+dropFirst`, v.slice(3)]);
    shapes.push([`${name}+dropLast`, v.slice(0, -3)]);
    shapes.push([`${name}+dup`, [...v, v[0], v[1], v[2]]]);
    shapes.push([`${name}+nudge`, v.map((p, i) => (i === 0 ? [p[0] + 0.01, p[1], p[2]] : p))]);
    shapes.push([`${name}+tinyNudge`, v.map((p, i) => (i === 0 ? [p[0] + 1e-5, p[1], p[2]] : p))]);
    shapes.push([`${name}+degenerate`, [...v, v[0], v[0], v[1]]]);
    shapes.push([`${name}+reversedOrder`, [...v].reverse()]);
  }
}
shapes.push(['negzero', [[-0, 0, 0], [0, -0, 0], [0, 0, -0]]]);
shapes.push(['nan', [[NaN, 0, 0], [0, 1, 0], [0, 0, 1]]]);
shapes.push(['large', [[1e20, 0, 0], [0, 1e21, 0], [0, 0, 1e-9], [5e-4, 5e-4, 5e-4], [-5e-4, 1.5e-3, 2.5e-3], [1, 1, 1]]]);
const cases = (fn) => shapes.map(([n, v]) => ({ id: n, args: [v], out: fn(v) }));
writeCases('testkit', 'isClosed', cases(isClosed));
writeCases('testkit', 'isOriented', cases(isOriented));
writeCases('testkit', 'bbox', cases(bbox));

// tineBoxes: runs of 36-vertex boxes, with and without wall steps (and a trailing partial box)
const bv = (...a) => verts(block(...a));
const tineRuns = {
  none: [...bv(0, 2, 0, 2, 5, 9), ...bv(3, 5, 0, 2, 5, 9), ...bv(6, 8, 0, 2, 5, 9)],
  steps: [...bv(0, 2, 0, 2, 5, 9), ...bv(0.5, 1.5, 0.5, 1.5, 0, 6), ...bv(3, 5, 0, 2, 5, 9),
    ...bv(6, 8, 0, 2, 5, 9), ...bv(6, 8, 0, 2, 1, 7)],
  twoSteps: [...bv(0, 2, 0, 2, 5, 9), ...bv(0.5, 1.5, 0.5, 1.5, 0, 6), ...bv(0.5, 1.5, 0.5, 1.5, 0, 6)],
  edgeTol: [...bv(0, 2, 0, 2, 5, 9), ...bv(0, 2 + 5e-7, 0, 2, 0, 6), ...bv(0, 2 + 2e-6, 0, 2, 0, 6)],
  partial: [...bv(0, 2, 0, 2, 5, 9), ...bv(0, 1, 0, 1, 0, 6).slice(0, 35)],
  notStep: [...bv(0, 2, 0, 2, 5, 9), ...bv(0.5, 1.5, 0.5, 1.5, 5, 6), ...bv(0.5, 1.5, 0.5, 1.5, 0, 10),
    ...bv(0.5, 1.5, 0.5, 1.5, 0, 5), ...bv(1.5, 2.5, 0.5, 1.5, 0, 6)],
  empty: [],
};
writeCases('testkit', 'tineBoxes', Object.entries(tineRuns).map(([n, v]) => {
  const r = tineBoxes(v);
  const tines = r.tines;
  const steps = r.steps.map((s) => ({ ...s, tine: tines.indexOf(s.tine) }));
  return { id: n, args: [v], out: { tines, steps } };
}));
