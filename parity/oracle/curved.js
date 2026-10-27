// Oracle recorder for web/prop/curved.js -> parity/corpus/curved/<fn>.jsonl. Replayed in
// crates/sf-engine/tests/parity_curved.rs with Tol::Exact.
// Each case ends with the {"$config": {"CURVED": {...}}} snapshot.
//   line_bend  args: [line, $config]            out: number (deg)
//   bends      args: [walls, $config]           out: boolean; walls = [{line}, ...] (a wall may lack `line`)
import { assertPinned, WEB } from './lib/pin.js';
import { encode, writeCases } from './lib/encode.js';

assertPinned();
const { lineBend, bends, CURVED } = await import(`${WEB}prop/curved.js`);

function rng(seed) {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13; s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5; s >>>= 0;
    return s / 4294967296;
  };
}
const R = rng(0xc0ffee11);
const rnd = (lo, hi) => lo + R() * (hi - lo);

const lines = [];
const pts = (n, f) => Array.from({ length: n }, (_, i) => f(i));
lines.push([], [[0, 0, 1]], [[0, 0, 1], [1, 0, 1]]);
lines.push(pts(30, (i) => [i, 0, 5]), pts(30, (i) => [i, 0.01 * i * i / 10, 5]));
for (const turn of [5, 9.9, 10.1, 30, 90, 180, 270, 360, 540, 720]) { // total turn in degrees, radius 15
  for (const step of [0.5, 1, 2]) {
    const n = Math.ceil(((turn * Math.PI) / 180) * 15 / step) + 1;
    lines.push(pts(n, (i) => {
      const a = (turn * Math.PI / 180) * (i / (n - 1));
      return [15 * Math.cos(a), 15 * Math.sin(a), 4 + 0.1 * i];
    }));
    lines.push(pts(n, (i) => {
      const a = -(turn * Math.PI / 180) * (i / (n - 1)) + Math.PI; // clockwise, heading wraps through +-pi
      return [15 * Math.cos(a), 15 * Math.sin(a), 4];
    }));
  }
}
lines.push(pts(60, (i) => [i * 0.5, 4 * Math.sin(i / 6), 3]));                 // S curve
lines.push(pts(60, (i) => [i * 0.5, (i % 2) * 0.3, 3]));                        // wobble
lines.push(pts(40, (i) => [Math.sin(i) * 0.05 + i * 0.7, Math.cos(i) * 0.05, 3])); // jitter on a line
lines.push(pts(25, (i) => [i * i * 0.1, 0, 3]));                                 // uneven spacing
lines.push([[0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0]], [[0, 0, 0], [5, 0, 0], [5, 5, 0], [0, 5, 0], [0, 0, 0]]);
lines.push([[0, 0, 0], [6, 0, 0], [6, 0.0001, 0], [12, 0, 0]], [[0, 0, 0], [NaN, 0, 0], [6, 0, 0], [12, 0, 0]]);
for (let i = 0; i < 40; i++) lines.push(pts(5 + Math.floor(R() * 40), () => [rnd(-10, 10), rnd(-10, 10), rnd(0, 5)]));
for (let i = 0; i < 20; i++) { // random walks with a steady heading change
  let x = 0, y = 0, h = rnd(0, 6), k = rnd(-0.3, 0.3);
  lines.push(pts(10 + Math.floor(R() * 40), () => { const p = [x, y, 1]; h += k; x += Math.cos(h) * rnd(0.3, 1.5); y += Math.sin(h) * rnd(0.3, 1.5); return p; }));
}

const variants = [{}, { chord: 1 }, { chord: 6, trim: 0 }, { trim: 0.4 }, { trim: 0.5 }, { minBend: 45 }];
const cfgArg = () => ({ $config: { CURVED: encode({ ...CURVED }) } });

const bendCases = [], bendsCases = [];
variants.forEach((v, vi) => {
  const was = { ...CURVED };
  Object.assign(CURVED, v);
  lines.forEach((line, i) => {
    if (vi && i % 3) return;
    bendCases.push({ id: `v${vi}/${i}`, args: [encode(line), cfgArg()], out: encode(lineBend(line)) });
  });
  for (let i = 0; i + 3 < lines.length; i += 2) {
    const walls = [{ line: lines[i] }, { line: lines[i + 1] }, {}, { line: lines[i + 3] }].slice(0, 1 + (i % 4));
    if (vi && i % 4) continue;
    bendsCases.push({ id: `v${vi}/${i}`, args: [encode(walls), cfgArg()], out: bends(walls) });
  }
  Object.assign(CURVED, was);
});
writeCases('curved', 'line_bend', bendCases, { encoded: true });
writeCases('curved', 'bends', bendsCases.map((c) => ({ ...c, out: encode(c.out) })), { encoded: true });
