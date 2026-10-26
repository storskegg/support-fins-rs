// Recorder for crates/sf-engine/src/jsnum.rs. Run with `just oracle jsnum`.
//
// Every function in jsnum.rs gets a file parity/corpus/jsnum/<rust fn name>.jsonl.
// Each case is {id, args, out}: `args` are the Rust function's arguments in order. The slice
// functions (min_of, max_of, hypot) take ONE argument, an array of numbers, and the recorder
// spreads it into the real Math.* call. `ushr` is `a >>> b`, `imul` is Math.imul.
// to_int32 / to_uint32 are `x | 0` / `x >>> 0`; string_to_number is Number(s).
//
// Inputs are deterministic: special values, their one-ulp neighbours, then 5,000 values from a
// fixed-seed xorshift64 (half over raw bit patterns, half uniform over a range meaningful to
// the function). Nothing here uses Math.random or the clock.
import { assertPinned } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();

// ------------------------------------------------------------ deterministic inputs

const MASK = (1n << 64n) - 1n;
let state = 0x9e3779b97f4a7c15n;
function nextU64() {
  // xorshift64 (13, 7, 17)
  state ^= (state << 13n) & MASK;
  state ^= state >> 7n;
  state ^= (state << 17n) & MASK;
  return state;
}
function reseed(seed) {
  state = (0x9e3779b97f4a7c15n ^ BigInt(seed)) & MASK || 1n;
  for (let i = 0; i < 8; i++) nextU64();
}
const dv = new DataView(new ArrayBuffer(8));
function fromBits(b) {
  dv.setBigUint64(0, b);
  return dv.getFloat64(0);
}
function toBits(x) {
  dv.setFloat64(0, x);
  return dv.getBigUint64(0);
}
function nextUp(x) {
  if (Number.isNaN(x) || x === Infinity) return x;
  if (x === 0) return 5e-324;
  const b = toBits(x);
  return fromBits(x > 0 ? b + 1n : b - 1n);
}
function nextDown(x) {
  return -nextUp(-x);
}
/** a raw bit pattern, any of 2^64 (so NaNs and infinities occur at their natural rate) */
function rawBits() {
  return fromBits(nextU64());
}
/** uniform in [0, 1) with 53 random bits */
function unit() {
  return Number(nextU64() >> 11n) / 9007199254740992;
}
function uniform(lo, hi) {
  return lo + unit() * (hi - lo);
}
/** log-uniform magnitude in [lo, hi] (both > 0) */
function logUniform(lo, hi) {
  const a = Math.log(lo);
  const b = Math.log(hi);
  return Math.exp(a + unit() * (b - a));
}

const SPECIAL = [
  0, -0, NaN, Infinity, -Infinity,
  5e-324, -5e-324,
  2.2250738585072014e-308, -2.2250738585072014e-308,
  Number.MAX_VALUE, -Number.MAX_VALUE,
  0.5, -0.5, 1.5, -1.5, 2.5, -2.5,
  1, -1, 2, -2, 3, 10, 100,
  2 ** 31 - 1, -(2 ** 31 - 1), 2 ** 31, -(2 ** 31), 2 ** 32, -(2 ** 32),
  2 ** 52, -(2 ** 52), 2 ** 53, -(2 ** 53), 2 ** 53 + 2, -(2 ** 53 + 2),
  1e21, -1e21, 1e-7, -1e-7, 0.1 + 0.2,
  Math.PI, -Math.PI, Math.PI / 2, 10 * Math.PI, -10 * Math.PI, Math.E,
  1e20, 1e-6, 123456789.123456789,
];

function idOf(x) {
  if (Array.isArray(x)) return '[' + x.map(idOf).join(';') + ']';
  if (typeof x === 'string') return JSON.stringify(x);
  if (Object.is(x, -0)) return '-0';
  return String(x);
}

/** dedupe by id, keeping first occurrence; builds the case list */
function makeCases(inputs, run) {
  const seen = new Set();
  const cases = [];
  for (const args of inputs) {
    const id = args.map(idOf).join(',');
    if (seen.has(id)) continue;
    seen.add(id);
    cases.push({ id, args, out: run(...args) });
  }
  return cases;
}

/** specials plus each special's neighbours one ulp up and down */
function specialsWithNeighbours(extra = []) {
  const out = [];
  for (const s of [...SPECIAL, ...extra]) {
    out.push(s);
    if (!Number.isNaN(s)) {
      out.push(nextUp(s), nextDown(s));
    }
  }
  return out;
}

/** 5,000 random values: 2,500 raw bit patterns, 2,500 from `meaningful()` */
function randomValues(seed, meaningful, n = 5000) {
  reseed(seed);
  const out = [];
  for (let i = 0; i < n / 2; i++) out.push(rawBits());
  for (let i = 0; i < n / 2; i++) out.push(meaningful(i));
  return out;
}

let seedCounter = 1;
/** inputs for a unary function: specials + neighbours + `extra` + 5,000 random */
function unaryInputs(meaningful, extra = []) {
  const seed = seedCounter++;
  return [...specialsWithNeighbours(), ...extra, ...randomValues(seed, meaningful)].map((x) => [x]);
}

/** pairs: the cross product of base specials, then 5,000 random pairs */
function pairInputs(meaningfulPair, extra = []) {
  const seed = seedCounter++;
  const out = [];
  for (const a of SPECIAL) for (const b of SPECIAL) out.push([a, b]);
  // neighbours of each special against a few fixed partners
  for (const a of specialsWithNeighbours()) {
    for (const b of [0, -0, 1, -1, 2, 0.5, NaN, Infinity, -Infinity, 3.5]) {
      out.push([a, b], [b, a]);
    }
  }
  out.push(...extra);
  reseed(seed);
  for (let i = 0; i < 2500; i++) out.push([rawBits(), rawBits()]);
  for (let i = 0; i < 2500; i++) out.push(meaningfulPair(i));
  return out;
}

const sym = (r) => () => uniform(-r, r);

function record(name, inputs, run) {
  writeCases('jsnum', name, makeCases(inputs, run));
}

// ------------------------------------------------------------ rounding, sign, abs

const rangeMixed = (i) => {
  // half integers and fractions around many magnitudes, so round/floor/ceil/trunc hit .5 cases
  switch (i % 4) {
    case 0: return uniform(-1e6, 1e6);
    case 1: return Math.floor(uniform(-1e6, 1e6)) + 0.5;
    case 2: return uniform(-10, 10);
    default: return uniform(-1, 1) * 2 ** Math.floor(uniform(0, 53));
  }
};
const halves = [];
for (let k = -40; k <= 40; k++) halves.push(k, k + 0.5, k - 0.5, k + 0.25, k + 0.75);
const bigHalves = [];
for (let e = 40; e <= 54; e++) for (const d of [-1.5, -1, -0.5, 0, 0.5, 1, 1.5]) bigHalves.push(2 ** e + d, -(2 ** e + d));

record('round', unaryInputs(rangeMixed, [...halves, ...bigHalves]), (x) => Math.round(x));
record('trunc', unaryInputs(rangeMixed, [...halves, ...bigHalves]), (x) => Math.trunc(x));
record('floor', unaryInputs(rangeMixed, [...halves, ...bigHalves]), (x) => Math.floor(x));
record('ceil', unaryInputs(rangeMixed, [...halves, ...bigHalves]), (x) => Math.ceil(x));
record('sign', unaryInputs(rangeMixed), (x) => Math.sign(x));
record('abs', unaryInputs(rangeMixed), (x) => Math.abs(x));

// ------------------------------------------------------------ min / max

const pairMixed = () => [rangeMixed(Number(nextU64() & 3n)), rangeMixed(Number(nextU64() & 3n))];
record('min', pairInputs(pairMixed), (a, b) => Math.min(a, b));
record('max', pairInputs(pairMixed), (a, b) => Math.max(a, b));

/** arrays for the variadic functions: hand-picked, then 5,000 random */
function arrayInputs(seed) {
  const out = [[], [0], [-0], [NaN], [Infinity], [-Infinity], [0, -0], [-0, 0], [-0, -0], [0, 0],
    [NaN, Infinity], [Infinity, NaN], [-Infinity, NaN], [NaN, 1], [1, NaN, 2],
    [3, 4], [1, 1, 1], [1e200, 1e200], [1e-200, 1e-200], [5e-324, 5e-324], [Number.MAX_VALUE, Number.MAX_VALUE],
    [3, 4, 12], [1, 2, 3, 4, 5, 6, 7, 8, 9, 10], [-5, 5], [2, -2, 2, -2],
    [1e308, 1e308, 1e308], [1e-320, 1e-320, 1e-320], [0.1, 0.2, 0.3], [-0, NaN], [NaN, -0]];
  for (const s of SPECIAL) out.push([s], [s, s], [s, 1], [1, s], [s, -s]);
  reseed(seed);
  for (let i = 0; i < 5000; i++) {
    const len = Number(nextU64() % 9n);
    const a = [];
    for (let j = 0; j < len; j++) {
      const k = Number(nextU64() & 7n);
      if (k === 0) a.push(rawBits());
      else if (k === 1) a.push(SPECIAL[Number(nextU64() % BigInt(SPECIAL.length))]);
      else if (k === 2) a.push(logUniform(1e-310, 1e300) * (unit() < 0.5 ? -1 : 1));
      else if (k === 3) a.push(Math.round(uniform(-20, 20)));
      else a.push(uniform(-1e3, 1e3));
    }
    out.push(a);
  }
  return out.map((a) => [a]);
}
record('min_of', arrayInputs(101), (a) => Math.min(...a));
record('max_of', arrayInputs(102), (a) => Math.max(...a));
record('hypot', arrayInputs(103), (a) => Math.hypot(...a));

// ------------------------------------------------------------ transcendental

const sqrtRange = (i) => (i % 2 ? logUniform(1e-300, 1e300) : uniform(0, 1e6));
record('sqrt', unaryInputs(sqrtRange), (x) => Math.sqrt(x));

const trigRange = (i) => (i % 2 ? uniform(-1e6, 1e6) : uniform(-10 * Math.PI, 10 * Math.PI));
// multiples of pi/2 and pi/4 are where range reduction is hardest
const piMultiples = [];
for (let k = -64; k <= 64; k++) {
  const m = k * (Math.PI / 4);
  piMultiples.push(m, nextUp(m), nextDown(m));
}
for (const f of ['sin', 'cos', 'tan']) {
  record(f, unaryInputs(trigRange, piMultiples), (x) => Math[f](x));
}

const unitRange = (i) => (i % 2 ? uniform(-1, 1) : (unit() < 0.5 ? -1 : 1) * logUniform(1e-20, 1));
const unitEdges = [];
for (const e of [1, -1, 0.5, -0.5, 0.9999999999999999, -0.9999999999999999, 1.0000000000000002, -1.0000000000000002]) {
  unitEdges.push(e);
}
record('asin', unaryInputs(unitRange, unitEdges), (x) => Math.asin(x));
record('acos', unaryInputs(unitRange, unitEdges), (x) => Math.acos(x));
const atanRange = (i) => (i % 2 ? uniform(-100, 100) : (unit() < 0.5 ? -1 : 1) * logUniform(1e-20, 1e20));
record('atan', unaryInputs(atanRange), (x) => Math.atan(x));
const atan2Pair = (i) => (i % 2 ? [uniform(-100, 100), uniform(-100, 100)]
  : [(unit() < 0.5 ? -1 : 1) * logUniform(1e-20, 1e20), (unit() < 0.5 ? -1 : 1) * logUniform(1e-20, 1e20)]);
record('atan2', pairInputs(atan2Pair), (y, x) => Math.atan2(y, x));

const expRange = (i) => (i % 2 ? uniform(-750, 750) : uniform(-1, 1));
record('exp', unaryInputs(expRange, [709.782712893384, 709.7827128933841, -745.1332191019411, -745.1332191019412, -708.3964185322641]), (x) => Math.exp(x));

const logRange = (i) => {
  switch (i % 3) {
    case 0: return uniform(0, 1e300);
    case 1: return logUniform(1e-300, 1e300);
    default: return uniform(0, 10);
  }
};
const exactPowers = [];
for (let k = -1074; k <= 1023; k++) exactPowers.push(2 ** k);
for (let k = -323; k <= 308; k++) exactPowers.push(Number('1e' + k));
const nearOne = [];
for (let k = -40; k <= 40; k++) nearOne.push(1 + k * 2 ** -52, 1 + k * 2 ** -30, 1 + k * 1e-3);
record('log', unaryInputs(logRange, [...exactPowers, ...nearOne]), (x) => Math.log(x));
record('log2', unaryInputs(logRange, [...exactPowers, ...nearOne]), (x) => Math.log2(x));
record('log10', unaryInputs(logRange, [...exactPowers, ...nearOne]), (x) => Math.log10(x));

const cbrtRange = (i) => (i % 2 ? uniform(-1e6, 1e6) : (unit() < 0.5 ? -1 : 1) * logUniform(1e-300, 1e300));
const cubes = [];
for (let n = -60; n <= 60; n++) cubes.push(n ** 3, n ** 3 + 1, n ** 3 - 1);
record('cbrt', unaryInputs(cbrtRange, cubes), (x) => Math.cbrt(x));

// pow: pairs. Exponents include small integers and halves because the engine mostly uses those.
const powPair = (i) => {
  switch (i % 5) {
    case 0: return [uniform(0, 1e3), uniform(-30, 30)];
    case 1: return [uniform(-10, 10), Math.round(uniform(-20, 20))];
    case 2: return [uniform(0, 100), Math.round(uniform(-8, 8)) / 2];
    case 3: return [logUniform(1e-10, 1e10), uniform(-3, 3)];
    default: return [uniform(-1e3, 1e3), Math.round(uniform(0, 10))];
  }
};
const powExtra = [];
for (const b of [2, 10, 0.5, -2, 1.5, 4, 9, 16, 100, 0.1, Math.E, 1e-3]) {
  for (let e = -12; e <= 12; e++) powExtra.push([b, e], [b, e + 0.5], [b, e / 3]);
}
record('pow', pairInputs(powPair, powExtra), (x, y) => Math.pow(x, y));

// ------------------------------------------------------------ integer coercion

const intRange = (i) => {
  switch (i % 4) {
    case 0: return uniform(-(2 ** 33), 2 ** 33);
    case 1: return uniform(-1e10, 1e10);
    case 2: return Math.round(uniform(-(2 ** 33), 2 ** 33)) + 0.5;
    default: return Math.round(uniform(-(2 ** 53), 2 ** 53));
  }
};
const wrapPoints = [];
for (let k = -4; k <= 4; k++) for (const d of [-1, -0.5, 0, 0.5, 1, 0.999999]) wrapPoints.push(k * 2 ** 31 + d, k * 2 ** 32 + d);
record('to_int32', unaryInputs(intRange, wrapPoints), (x) => x | 0);
record('to_uint32', unaryInputs(intRange, wrapPoints), (x) => x >>> 0);
const intPair = () => [intRange(Number(nextU64() & 3n)), intRange(Number(nextU64() & 3n))];
record('imul', pairInputs(intPair, [[0xffffffff, 5], [65536, 65536], [2 ** 31, 2], [-1, 2 ** 31], [0xffffffff, 0xffffffff]]), (a, b) => Math.imul(a, b));
const ushrPair = () => [intRange(Number(nextU64() & 3n)), Math.round(uniform(-70, 70))];
record('ushr', pairInputs(ushrPair, [[-1, 0], [-1, 1], [-1, 31], [-1, 32], [-1, 33], [2 ** 32, 1], [1, 31], [1, -1]]), (a, b) => a >>> b);

// ------------------------------------------------------------ number <-> string

const strRange = (i) => {
  switch (i % 5) {
    case 0: return uniform(-1e6, 1e6);
    case 1: return logUniform(1e-30, 1e30) * (unit() < 0.5 ? -1 : 1);
    case 2: return Math.round(uniform(-1e9, 1e9));
    case 3: return Number((uniform(-1e4, 1e4)).toFixed(3));
    default: return uniform(-1, 1) * 10 ** Math.round(uniform(-10, 25));
  }
};
const strEdges = [1e21, 1e21 - 1, 999999999999999900000, 1e-6, 1e-7, 9.9e-7, 1.5e-7, 123456789012345680000,
  0.000001, 0.0000001, 100, 1e5, 1e20, 12345.6789, 4.35, 0.5, 1.005, 2.5, 8.345, 1.45];
record('to_js_string', unaryInputs(strRange, strEdges), (x) => String(x));

// to_fixed: digits 0..=20, 500 values each (specials first, then random fill)
{
  const inputs = [];
  const base = specialsWithNeighbours([...strEdges, 1.005, 1.255, 8.345, 10.235, 0.000001, 0.5, 1.5, 2.5, 1e-10, 123.456, 0.045]);
  for (let digits = 0; digits <= 20; digits++) {
    reseed(1000 + digits);
    const vals = base.slice(0, 500);
    while (vals.length < 500) {
      const k = vals.length % 4;
      vals.push(k === 0 ? rawBits() : k === 1 ? uniform(-1e6, 1e6) : k === 2 ? uniform(-10, 10) : logUniform(1e-10, 1e22) * (unit() < 0.5 ? -1 : 1));
    }
    for (const v of vals) inputs.push([v, digits]);
  }
  // digits above 20 too (JS allows 0..=100): specials and neighbours only
  for (const digits of [21, 25, 30, 50, 99, 100]) for (const v of base) inputs.push([v, digits]);
  record('to_fixed', inputs, (x, d) => x.toFixed(d));
}

// string_to_number and parse_float: the to_js_string outputs, then a hand list
const numericStrings = (() => {
  const set = new Set();
  const nums = [...specialsWithNeighbours(), ...randomValues(900, strRange, 3000)];
  for (const n of nums) set.add(String(n));
  for (const n of nums.slice(0, 600)) {
    set.add(n.toExponential());
    set.add(' ' + String(n) + ' ');
    set.add(String(n) + 'px');
    if (Number.isFinite(n)) set.add(n.toFixed(3));
  }
  return [...set];
})();
const handStrings = ['  1.5e3 ', '0x1F', 'Infinity', '-0', '', '1e', '.5', '5.', '1_000',
  '+Infinity', '-Infinity', 'infinity', 'INFINITY', 'Infinityx', 'NaN', 'nan', '+', '-', '.', '+.5', '-.5e1', '5.e2', '.e2',
  '0x', '0X1f', '0xg', '-0x1F', '+0x1F', '0o17', '0O17', '0b101', '0B101', '0b2', '0o8', '0x1.8', '0xFFFFFFFFFFFFFFFFFF',
  '0b' + '1'.repeat(80), '0o' + '7'.repeat(40),
  '1e5', '1E5', '1e+5', '1e-5', '1e+', '1e-', '1e5.5', '1.5.5', '1..5', '1,5', '--1', '+-1', '1 2', '1 2',
  ' \t\n\r\v\f                 　﻿12　﻿',
  '​1', '᠎1', '1​', '٣', '１２', '1e1000', '-1e1000', '1e-1000', '0e0', '00012', '-00012.50', '0.0000001', '1e21',
  '123456789012345678901234567890', '9007199254740993', '9007199254740992.5', '4.9e-324', '2e-324', '2.4703282292062328e-324', '2.4703282292062327e-324',
  '1.7976931348623157e308', '1.7976931348623159e308', '1.797693134862315807e308', '1.7976931348623158e308',
  '0.1', '0.30000000000000004', '1e23', '8.41e21', '5e-324', '4.9406564584124654e-324',
  '0.' + '0'.repeat(400) + '1', '1' + '0'.repeat(400), '1' + '0'.repeat(400) + 'e-400',
  '3.14159abc', '  42  ', '42abc', 'abc', 'e5', 'Infinity1', '1Infinity', '  Infinity  ', '+ 1', '1 e5', '0x 1', '0_1', '1__0', '१२३'];
const allStrings = [...new Set([...handStrings, ...numericStrings])];
record('string_to_number', allStrings.map((s) => [s]), (s) => Number(s));
record('parse_float', allStrings.map((s) => [s]), (s) => parseFloat(s));
