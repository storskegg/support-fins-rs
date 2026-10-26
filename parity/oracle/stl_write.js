// Oracle recorder for writeBinarySTL (web/stl.js:22-63) -> parity/corpus/stl/writeBinarySTL.jsonl.
//
// args: [tris, name] where tris is a flat array of [x, y, z] vertices (three per triangle);
// out: the bytes of the Blob it returns (Uint8Array). Replayed in
// crates/sf-io/tests/parity_stl_write.rs with Tol::Exact (byte for byte).
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { assertPinned, UPSTREAM_ROOT, WEB } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();
const { writeBinarySTL, readSTL } = await import(`${WEB}stl.js`);

const cases = [];
const add = async (id, tris, name) => {
  const out = new Uint8Array(await writeBinarySTL(tris, name).arrayBuffer());
  cases.push({ id, args: [tris, name], out });
};
const verts = (soup, maxTris = Infinity) => {
  const v = [];
  for (let i = 0; i + 2 < soup.length && v.length < maxTris * 3; i += 3) v.push([soup[i], soup[i + 1], soup[i + 2]]);
  return v;
};

// --- crafted triangles --------------------------------------------------------------------
const T = [[0, 0, 0], [1, 0, 0], [0, 1, 0]];
await add('empty', [], 'Support Fins');
await add('one', T, 'Support Fins');
await add('two', [...T, [0, 0, 1], [0, 1, 1], [1, 0, 1]], 'Support Fins');
await add('partial-4', [...T, [5, 5, 5]], 'Support Fins');
await add('partial-5', [...T, [5, 5, 5], [6, 6, 6]], 'x');
await add('zero-area', [[1, 1, 1], [1, 1, 1], [1, 1, 1]], 'Support Fins');
await add('collinear', [[0, 0, 0], [1, 1, 1], [2, 2, 2]], 'Support Fins');
await add('nan', [[NaN, 0, 0], [1, 0, 0], [0, 1, 0]], 'Support Fins');
await add('nan-second', [[0, 0, 0], [1, 0, NaN], [0, 1, 0]], 'Support Fins');
await add('infinity', [[0, 0, 0], [Infinity, 0, 0], [0, 1, 0]], 'Support Fins');
await add('neg-infinity-z', [[0, 0, 0], [1, 0, -Infinity], [0, 1, 0]], 'Support Fins');
await add('neg-zero', [[-0, -0, -0], [1, -0, 0], [0, 1, -0]], 'Support Fins');
await add('f32-overflow', [[0, 0, 0], [1e40, 0, 0], [0, -1e40, 1e-50]], 'Support Fins');
await add('f32-rounding', [[0.1, 0.2, 0.3], [16777217, 1e-46, 3.4028235677973366e38], [1.17549435e-38, -1e-7, 123456789.123]], 'Support Fins');
// normal length around the 1e-12 threshold: edges of 1e-6 give a cross product near 1e-12
for (const e of [1e-6, 0.99999e-6, 1.00001e-6, 5e-7, 2e-6]) {
  await add(`threshold-${e}`, [[0, 0, 0], [e, 0, 0], [0, e, 0]], 'Support Fins');
}
await add('tiny', [[0, 0, 0], [1e-200, 0, 0], [0, 1e-200, 0]], 'Support Fins');
await add('huge', [[0, 0, 0], [1e200, 0, 0], [0, 1e200, 0]], 'Support Fins');
await add('huge-overflow-hypot', [[0, 0, 0], [1e160, 0, 0], [0, 1e160, 1e160]], 'Support Fins');
await add('negative', [[-3.5, -2.25, -1.125], [4.75, -7.5, 2], [0.1, 9.9, -8.8]], 'Support Fins');

// --- header names -------------------------------------------------------------------------
for (const [k, name] of Object.entries({
  default: 'Support Fins', empty: '', short: 'a', v123: 'v 1 2 3', exact79: 'x'.repeat(79 - ' - support-fins'.length),
  longer: 'x'.repeat(200), emdash: 'part — fins', accents: 'café üñî', cjk: '中文零件',
  emoji: 'pin \u{1F600} x', emojiAt79: 'y'.repeat(79 - ' - support-fins'.length - 1) + '\u{1F600}', control: 'a\tb\nc\u0000d\u007fe',
  highbit: 'ĀƀȀ', bom: '﻿bom',
})) await add(`name-${k}`, T, name);

// --- fixtures and stress models (first triangles; whole file when small) -------------------
function walk(dir, out = []) {
  for (const n of readdirSync(dir).sort()) {
    const p = join(dir, n);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, out);
    else if (/\.stl$/i.test(n) && st.size < 2 * 1024 * 1024) out.push([p, st.size]);
  }
  return out;
}
for (const root of ['tests/fixtures', 'prototype/stress/models']) {
  for (const [p, size] of walk(join(UPSTREAM_ROOT, root))) {
    const soup = readSTL(new Uint8Array(readFileSync(p)));
    const rel = relative(UPSTREAM_ROOT, p);
    await add(`file-head:${rel}`, verts(soup, 40), `${rel.split('/').pop().replace(/\.stl$/i, '')}`);
    if (size < 30000) await add(`file-whole:${rel}`, verts(soup), 'Support Fins');
  }
}

// --- 600 pseudo-random triangles (fixed seed) ---------------------------------------------
let seed = 0x1badf00d;
const rnd = () => {
  seed = (seed + 0x6D2B79F5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const rv = () => [(rnd() - 0.5) * 200, (rnd() - 0.5) * 200, (rnd() - 0.5) * 200];
const rtris = [];
for (let i = 0; i < 600 * 3; i++) rtris.push(rv());
await add('random-600', rtris, 'Random');

writeCases('stl', 'writeBinarySTL', cases);
