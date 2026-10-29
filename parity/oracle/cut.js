// Oracle recorder for web/cut.js -> parity/corpus/cut/cutNegatives.jsonl.
// args: [positions (Float32Array), colors | null, extras [{kind, name, positions}]]
// out:  { positions (Float32Array), colors, cut, uncut, why }
// The cut is Manifold's (the vendored WASM, 3.5.4); the Rust replay (crates/sf-cut/tests/parity_cut.rs)
// runs the same Manifold built from source and must give the same mesh, position for position and
// triangle for triangle (Exact).
import { assertPinned, WEB, UPSTREAM_ROOT } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();
const { cutNegatives } = await import(`${WEB}cut.js`);
const { readThreeMF } = await import(`${WEB}threemf.js`);
const { readSTL } = await import(`${WEB}stl.js`);

const FIX = `${UPSTREAM_ROOT}/tests/fixtures/negative-parts/`;
const MODELS = `${UPSTREAM_ROOT}/prototype/stress/models/`;

const block = (x0, x1, y0, y1, z0, z1) => {
  const v = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
  const f = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
  return new Float32Array(f.flatMap((t) => t.flatMap((i) => v[i])));
};
const cat = (...arrs) => {
  const out = new Float32Array(arrs.reduce((n, a) => n + a.length, 0));
  let o = 0;
  for (const a of arrs) { out.set(a, o); o += a.length; }
  return out;
};
/** A cylinder along z (a bore), closed. */
const cyl = (cx, cy, r, z0, z1, n = 32) => {
  const t = [];
  for (let i = 0; i < n; i++) {
    const a = (2 * Math.PI * i) / n, b = (2 * Math.PI * (i + 1)) / n;
    const p = [cx + r * Math.cos(a), cy + r * Math.sin(a)], q = [cx + r * Math.cos(b), cy + r * Math.sin(b)];
    t.push(cx, cy, z0, q[0], q[1], z0, p[0], p[1], z0);
    t.push(cx, cy, z1, p[0], p[1], z1, q[0], q[1], z1);
    t.push(p[0], p[1], z0, q[0], q[1], z0, q[0], q[1], z1);
    t.push(p[0], p[1], z0, q[0], q[1], z1, p[0], p[1], z1);
  }
  return new Float32Array(t);
};
const neg = (positions, name = 'n') => ({ kind: 'negative', name, positions });

const cases = [], provenance = [];
const add = async (id, positions, colors, extras) => {
  const out = await cutNegatives({ positions, colors, extras });
  cases.push({ id, args: [positions, colors, extras], out });
  // cutNegatives(obj, { provenance: true }): the geometry must not change (the replay checks it
  // against `out` above), and sourceFaces says where each triangle came from
  const traced = await cutNegatives({ positions, colors, extras }, { provenance: true });
  const { sourceFaces, cut, uncut, why } = traced;
  provenance.push({ id, args: [positions, colors, extras], out: { sourceFaces, cut, uncut, why, samePositions: traced.positions.length === out.positions.length && traced.positions.every((v, i) => Object.is(v, out.positions[i])) } });
};

// the slicers' own files
for (const f of ['bambu.3mf', 'orca.3mf', 'prusaslicer3.3mf']) {
  const o = (await readThreeMF(await Deno.readFile(`${FIX}${f}`))).objects[0];
  await add(`file:${f}`, o.positions, o.colors ?? null, o.extras);
}

// cut.test.js's synthetic parts
const cube = block(0, 10, 0, 10, 0, 10);
await add('modifier-only', cube, null, [{ kind: 'modifier', name: 'm', positions: block(2, 4, 2, 4, 2, 4) }]);
await add('shells+debris', block(0, 10, 0, 10, 0, 20), null,
  [neg(cat(block(-1, 5, -1, 5, 5, 15), block(5, 11, 5, 11, 5, 15), new Float32Array([5, 5, 25, 6, 5, 25, 5, 6, 25])))]);
await add('open-negative', cube, null, [neg(block(2, 8, 2, 8, -1, 5).slice(0, 9 * 8))]);
await add('open-part', cube.slice(0, 9 * 10), null, [neg(block(2, 8, 2, 8, -1, 5))]);
await add('part-debris', cat(cube, new Float32Array([20, 0, 0, 21, 0, 0, 20, 1, 0])), null, [neg(block(2, 8, 2, 8, -1, 5))]);
{
  const inside = block(5, 15, 5, 15, 5, 15);
  for (let i = 0; i < inside.length; i += 9) for (let k = 0; k < 3; k++) [inside[i + 3 + k], inside[i + 6 + k]] = [inside[i + 6 + k], inside[i + 3 + k]];
  await add('inside-out', cube, null, [neg(inside)]);
}
await add('whole', cube, null, [neg(block(-1, 11, -1, 11, -1, 11))]);
{
  const paint = [], rgb = [];
  for (let t = 0; t < 12; t++) { paint[t] = { name: 'paint_color', v: `${t.toString(16)}1` }; rgb[t] = '#FF0000'; }
  await add('paint-stroked', cube, { paint, rgb }, [neg(block(8, 11, -1, 11, 8, 11))]);
  const whole = [];
  for (let t = 0; t < 12; t++) whole[t] = { name: 'paint_color', v: '8' };
  await add('paint-whole', cube, { paint: whole, rgb: [] }, [neg(block(8, 11, -1, 11, 8, 11))]);
}

// real geometry: each stress model, bored and notched
for await (const e of Deno.readDir(MODELS)) {
  if (!e.name.endsWith('.stl')) continue;
  const pos = readSTL(await Deno.readFile(`${MODELS}${e.name}`));
  let lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < pos.length; i += 3) for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], pos[i + k]); hi[k] = Math.max(hi[k], pos[i + k]); }
  const c = lo.map((v, k) => (v + hi[k]) / 2), s = hi.map((v, k) => v - lo[k]);
  const r = Math.max(1, Math.min(s[0], s[1]) / 6);
  await add(`model:${e.name}:bore`, pos, null, [neg(cyl(c[0], c[1], r, lo[2] - 1, hi[2] + 1))]);
  await add(`model:${e.name}:notch`, pos, null, [neg(block(c[0], hi[0] + 1, lo[1] - 1, c[1], c[2], hi[2] + 1)), neg(cyl(lo[0], c[1], r, lo[2] - 1, hi[2] + 1), 'second')]);
}
writeCases('cut', 'cutNegatives', cases);
// out: { sourceFaces (Int32Array), cut, uncut, why, samePositions }
writeCases('cut', 'cutNegativesProvenance', provenance);
