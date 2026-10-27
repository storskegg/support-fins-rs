// Oracle recorder for web/paintcode.js -> parity/corpus/paintcode/*.jsonl.
//   wholeCode      args [n]            out string | null
//   isWhole        args [code]         out boolean
//   fillUnpainted  args [code, n]      out string | null
//   filamentPainter  args [name, calls] where calls = [[p, n], ...] with p = null or {name, v};
//                  the painter is created once and called in order; out = the list of results
//                  ({name, v} or null).
import { assertPinned, WEB } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();
const { wholeCode, isWhole, fillUnpainted, filamentPainter } = await import(`${WEB}paintcode.js`);

const uniq = (cs) => { const s = new Set(); return cs.filter((c) => (s.has(c.id) ? false : (s.add(c.id), true))); };
const ns = [-1, 0, 0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4, 5, 10, 17, 18, 18.5, 19, 20, 100, NaN, Infinity, -Infinity, -0];
writeCases('paintcode', 'wholeCode', ns.map((n) => ({ id: `n${Object.is(n, -0) ? '-0' : String(n)}`, args: [n], out: wholeCode(n) })));

// codes: hand-made, every string of hex digits up to length 3, and random longer ones
const codes = ['', '0', '4', '8', '0C', '1C', 'FC', '801', '8', '1', '2', '3', '00011', 'Z4', '04', '0x4', ' 4', '4 ', '4\n', 'ab', 'AB', 'aB', '00', '000', '0000', 'F', 'FF', 'FFF',
  '٤', '４', '4\u{1F600}'];
const hex = '0123456789ABCDEF';
for (const a of hex) { codes.push(a); for (const b of hex) { codes.push(a + b); for (const c of '048CF1') codes.push(a + b + c); } }
let seed = 0xc0de;
const rnd = () => { seed = (seed + 0x6D2B79F5) | 0; let t = Math.imul(seed ^ (seed >>> 15), 1 | seed); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
for (let i = 0; i < 1500; i++) {
  const len = 1 + Math.floor(rnd() * 8);
  let s = ''; for (let k = 0; k < len; k++) s += hex[Math.floor(rnd() * 16)];
  codes.push(s);
}
const uc = [...new Set(codes)];
writeCases('paintcode', 'isWhole', uc.map((c) => ({ id: JSON.stringify(c), args: [c], out: isWhole(c) })));
const fills = [];
for (const c of uc) for (const n of [1, 2, 3, 4, 18]) fills.push({ id: `${JSON.stringify(c)}|${n}`, args: [c, n], out: fillUnpainted(c, n) });
for (const c of ['801', '8', '00011', '4', '0C', '']) for (const n of [0, 0.5, 1.5, 2.5, 5, 10, 17, 19, NaN, Infinity, -1]) fills.push({ id: `${JSON.stringify(c)}|${String(n)}`, args: [c, n], out: fillUnpainted(c, n) });
writeCases('paintcode', 'fillUnpainted', uniq(fills));

const P = (name, v) => ({ name, v });
const runs = {
  basic: ['paint_color', [[null, 1], [null, 2], [null, 3], [null, 4], [null, 18], [null, 19], [null, 0]]],
  painted: ['mmu_segmentation', [[P('slic3rpe:mmu_segmentation', '801'), 3], [P('slic3rpe:mmu_segmentation', '801'), 1], [P('paint_color', '8'), 3], [P('paint_color', '1C'), 2]]],
  unparsable: ['paint_color', [[P('paint_color', 'Z4'), 2], [P('paint_color', ''), 2], [null, 0.5], [P('x', '4'), NaN]]],
  nonInteger: ['n', [[null, 2.5], [null, 1.5], [P('a', '00011'), 2.5]]],
  same: ['n', [[P('a', '4'), 2], [P('a', '4'), 1], [null, 1], [P('a', '8'), 2]]],
};
writeCases('paintcode', 'filamentPainter', Object.entries(runs).map(([id, [name, calls]]) => {
  const paint = filamentPainter(name);
  return { id, args: [name, calls], out: calls.map(([p, n]) => paint(p, n) ?? null) };
}));
