// Oracle recorder for web/drawarc.js -> parity/corpus/drawarc/<fn>.jsonl. Replayed in
// crates/sf-engine/tests/parity_drawarc.rs with Tol::Exact.
//   arc_through   args: [a, b, m]  (each [x, y, z])             out: {cx, cy, r, a0, sweep, toM} | null
//   arc_stations  args: [a, b, m, step, minPts]                 out: [[x, y, hint], ...] | null
import { assertPinned, WEB } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();
const { arcThrough, arcStations, DRAW_MIN_RADIUS } = await import(`${WEB}drawarc.js`);

function rng(seed) {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13; s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5; s >>>= 0;
    return s / 4294967296;
  };
}
const R = rng(0xa4c0a4c1);
const rnd = (lo, hi) => lo + R() * (hi - lo);

const triples = [];
// a quarter, half, three-quarter circle both ways
for (const [a0, a1, am] of [[0, 90, 45], [0, 180, 90], [0, 270, 135], [0, -90, -45], [0, -270, -135], [0, 90, 300], [90, 0, 45], [10, 350, 180], [350, 10, 0]]) {
  const p = (d, r = 20) => [r * Math.cos((d * Math.PI) / 180), r * Math.sin((d * Math.PI) / 180), 0];
  triples.push([p(a0), p(a1), p(am)]);
}
// collinear, near collinear (the 1e-9 threshold), a huge radius (the 1e4 threshold), repeated points
triples.push([[0, 0, 0], [10, 0, 0], [5, 0, 0]], [[0, 0, 0], [10, 0, 0], [5, 1e-12, 0]], [[0, 0, 0], [10, 0, 0], [5, 1e-3, 0]]);
triples.push([[0, 0, 0], [10, 0, 0], [5, 1e-3, 0]], [[0, 0, 0], [1e5, 1, 0], [5e4, 0.5, 0]]);
triples.push([[0, 0, 0], [0, 0, 0], [1, 1, 0]], [[0, 0, 0], [1, 0, 0], [1, 0, 0]], [[1, 1, 5], [1, 1, 5], [1, 1, 5]]);
triples.push([[0, 0, 0], [4, 0, 0], [2, DRAW_MIN_RADIUS, 0]], [[0, 0, 0], [2 * DRAW_MIN_RADIUS, 0, 0], [DRAW_MIN_RADIUS, DRAW_MIN_RADIUS, 0]]);
for (let i = 0; i < 300; i++) {
  triples.push([0, 1, 2].map(() => [rnd(-50, 50), rnd(-50, 50), rnd(0, 40)]));
}
for (let i = 0; i < 40; i++) { // far from the origin, where the circle's centre formula loses digits
  const c = [rnd(-5000, 5000), rnd(-5000, 5000)];
  triples.push([0, 1, 2].map(() => [c[0] + rnd(-5, 5), c[1] + rnd(-5, 5), rnd(0, 10)]));
}

writeCases('drawarc', 'arc_through', triples.map(([a, b, m], i) => ({ id: `${i}`, args: [a, b, m], out: arcThrough(a, b, m) })));

const stations = [];
triples.forEach(([a, b, m], i) => {
  for (const [step, minPts] of [[1, 3], [0.5, 8], [2.5, 2], [10, 5]]) {
    const out = arcStations(a, b, m, step, minPts);
    if (out && out.length > 60) continue; // keep the corpus small: a radius of thousands of mm is hundreds of stations
    stations.push({ id: `${i}/${step}/${minPts}`, args: [a, b, m, step, minPts], out });
  }
});
writeCases('drawarc', 'arc_stations', stations);
