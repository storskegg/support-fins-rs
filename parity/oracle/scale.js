// Oracle recorder for web/scale.js -> parity/corpus/scale/{scaleAbout,scaleFromPercent}.jsonl.
// scaleAbout:       args [Float32Array src, s, from, to]  (`to` is always passed: the default
//                   `to = from` is resolved here); out Float32Array.
// scaleFromPercent: args [v] where v is a number or a string; out number or null.
import { assertPinned, WEB } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();
const { scaleAbout, scaleFromPercent } = await import(`${WEB}scale.js`);

let seed = 0x5ca1e;
const rnd = () => {
  seed = (seed + 0x6D2B79F5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};

const about = [];
const add = (id, src, s, from, to = from) => about.push({ id, args: [src, s, from, to], out: scaleAbout(src, s, from, to) });
add('basic', new Float32Array([1, 2, 3, -1, 0, 5]), 2, [0, 0, 0]);
add('about-to', new Float32Array([1, 2, 3, -1, 0, 5]), 1.5, [0, 0, 0], [10, 20, 30]);
add('about-point', new Float32Array([10, 20, 30, 12, 20, 30]), 3, [10, 20, 30]);
add('empty', new Float32Array(0), 2, [0, 0, 0]);
add('partial', new Float32Array([1, 2, 3, 4, 5]), 2, [0, 0, 0]);
add('zero-scale', new Float32Array([1, 2, 3]), 0, [1, 1, 1], [5, 5, 5]);
add('negative-scale', new Float32Array([1, 2, 3]), -2, [0, 0, 0]);
add('nan-scale', new Float32Array([1, 2, 3]), NaN, [0, 0, 0]);
add('inf-scale', new Float32Array([1, 0, 3]), Infinity, [0, 0, 0]);
add('neg-zero', new Float32Array([-0, 0, -0]), 1, [0, 0, 0], [-0, -0, -0]);
add('overflow-f32', new Float32Array([3e38, -3e38, 1]), 10, [0, 0, 0]);
add('rounding', new Float32Array([0.1, 0.2, 0.3]), 1 / 3, [0.7, 0.8, 0.9], [0.11, 0.22, 0.33]);
add('f32-nan', new Float32Array([NaN, 1, 2]), 2, [0, 0, 0]);
for (let i = 0; i < 200; i++) {
  const n = 3 * (1 + (i % 7));
  const src = new Float32Array(n).map(() => (rnd() - 0.5) * 400);
  add(`rand${i}`, src, [0.1, 0.5, 1, 2.5, 10, 0.333, 3.14159][i % 7] * (1 + rnd()),
    [(rnd() - 0.5) * 50, (rnd() - 0.5) * 50, (rnd() - 0.5) * 50], i % 2 ? [rnd() * 9, rnd() * 9, rnd() * 9] : undefined);
}
writeCases('scale', 'scaleAbout', about.map((c) => (c.args[3] === undefined ? c : c)));

const pct = [];
const pv = (v) => pct.push({ id: `${typeof v}:${String(v)}`, args: [v], out: scaleFromPercent(v) });
for (const v of ['150', 100, '5', '5000', '33.333', '33.33', 10, 1000, 9.999, 1000.001, 0, -5, NaN, Infinity, -Infinity, '', ' ', 'abc',
  '12px', null, true, false, '0x20', '1e2', ' 50 ', 10.004, 10.005, 10.006, 1.005, 0.1, 99.995, 12.345, 33.335, 66.665, 250, '250.555']) pv(v);
for (let i = 0; i < 300; i++) pv(Math.round(rnd() * 120000) / 100 - 100);
writeCases('scale', 'scaleFromPercent', dedupe(pct));

function dedupe(cs) {
  const seen = new Set();
  return cs.filter((c) => (seen.has(c.id) ? false : (seen.add(c.id), true)));
}
