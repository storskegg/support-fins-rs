// Oracle recorder for web/ui/footprint.js and web/ui/platerow.js -> parity/corpus/footprint/*.jsonl.
// Grids are { x0, y0, w, h, bits: Uint8Array } (a bed also carries bx, by).
//   rasterize: args [Float32Array tris, Float64Array|null m]            out grid|null
//   union:     args [[grid|null, ...]]  (the JS takes them spread)       out grid|null
//   placed:    args [grid, [x, y]]                                       out grid
//   shared:    args [grid|null, grid|null]                               out [[x, y], ...]
//   dilate:    args [grid, r]                                            out grid
//   bedGrid:   args [bx, by]                                             out bed grid
//   take:      args [bed, grid|null]                                     out the bed after (take mutates it)
//   findSpot:  args [bed, grid, gap, step]                               out [dx, dy]|null
//   pack:      args [[grid|null, ...], bx, by, gap]                      out [[x, y]|null, ...]
//   area:      args [grid|null]                                          out number
//   dragStep:  args [at, to, me (grid), others ([placed grid, ...])]     out [x, y]; the JS callback is
//              clash(xy) = the cells `me` placed at xy shares with `others`, the way ui/arrange.js clashAt sums them
//   plateUnder: args [xy, active, count, pitch, bx, by]                  out {plate, xy}|null
import { assertPinned, WEB } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();
const fp = await import(`${WEB}ui/footprint.js`);
const { plateUnder } = await import(`${WEB}ui/platerow.js`);

let seed = 0xf007;
const rnd = () => {
  seed = (seed + 0x6D2B79F5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const between = (a, b) => a + (b - a) * rnd();
const clone = (g) => g && { ...g, bits: new Uint8Array(g.bits) };

const box = (x0, y0, x1, y1) => [
  x0, y0, 1, x1, y0, 1, x1, y1, 1,
  x0, y0, 1, x1, y1, 1, x0, y1, 1,
];
const f32 = (a) => new Float32Array(a);

// ---- rasterize
const ras = [];
const addRas = (id, tris, m = null) => ras.push({ id, args: [tris, m && new Float64Array(m)], out: fp.rasterize(tris, m) });
addRas('box-10x4', f32(box(0, 0, 10, 4)));
addRas('box-inside', f32(box(0.2, 0.2, 9.8, 3.8)));
addRas('sliver', f32([0.5, 0.5, 0, 20.5, 10.5, 0, 20.5, 10.5, 5]));
addRas('translate', f32(box(0.2, 0.2, 2.8, 2.8)), [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 30, -5, 0, 1]);
addRas('empty', f32([]));
addRas('partial', f32([1, 2, 3, 4, 5, 6, 7, 8]));
addRas('point', f32([3.5, 3.5, 0, 3.5, 3.5, 1, 3.5, 3.5, 2]));
addRas('row', f32([0, 0.5, 0, 9, 0.5, 0, 9, 0.9, 0]));
addRas('column', f32([0.5, 0, 0, 0.9, 7, 0, 0.5, 7, 0]));
addRas('negative', f32(box(-12.5, -7.25, -2.5, -0.25)));
addRas('on-lines', f32(box(-3, -3, 3, 3)));
addRas('rot90', f32(box(0.2, 0.2, 20.2, 5.2)), [0, 1, 0, 0, -1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
addRas('tilt', f32([0, 0, 0, 30, 5, 10, 10, 25, 20]), [0.8, 0.6, 0, 0, -0.6, 0.8, 0, 0, 0, 0, 1, 0, 4.5, -2.25, 3, 1]);
for (let i = 0; i < 200; i++) {
  const n = 1 + Math.floor(rnd() * 12);
  const span = [3, 12, 30, 60][i % 4];
  const t = [];
  for (let k = 0; k < n * 9; k++) t.push(between(-span, span));
  const th = between(0, 2 * Math.PI), c = Math.cos(th), s = Math.sin(th);
  const m = i % 3 === 0 ? null : [c, s, 0, 0, -s, c, 0, 0, 0, 0, 1, 0, between(-50, 50), between(-50, 50), between(0, 20), 1];
  addRas(`rand${i}`, f32(t), m);
}
writeCases('footprint', 'rasterize', ras);

// a random footprint grid, about its own spot
const randGrid = () => {
  const k = 1 + Math.floor(rnd() * 3), tris = [];
  for (let i = 0; i < k; i++) {
    const x = between(-30, 20), y = between(-30, 20);
    tris.push(...box(x, y, x + between(1, 30), y + between(1, 30)));
  }
  return fp.rasterize(f32(tris));
};
const grids = Array.from({ length: 40 }, randGrid);
const L = fp.union(fp.rasterize(f32(box(0.2, 0.2, 29.8, 9.8))), fp.rasterize(f32(box(0.2, 0.2, 9.8, 29.8))));
const cube = fp.rasterize(f32(box(0.2, 0.2, 9.8, 9.8)));

// ---- union
const uni = [];
const addUni = (id, list) => uni.push({ id, args: [list.map(clone)], out: fp.union(...list) });
addUni('none', []);
addUni('nulls', [null, null]);
addUni('one', [cube]);
addUni('one-and-null', [null, cube, null]);
addUni('L', [fp.rasterize(f32(box(0.2, 0.2, 29.8, 9.8))), fp.rasterize(f32(box(0.2, 0.2, 9.8, 29.8)))]);
for (let i = 0; i < 100; i++) addUni(`rand${i}`, [grids[i % 40], grids[(i * 7 + 3) % 40], i % 4 === 0 ? null : grids[(i * 11 + 5) % 40]]);
writeCases('footprint', 'union', uni);

// ---- placed
const pl = [];
const addPl = (id, g, xy) => pl.push({ id, args: [clone(g), xy], out: fp.placed(g, xy) });
addPl('whole', cube, [3, -4]);
addPl('x-off', cube, [10.3, 0]);
addPl('both-off', cube, [-2.5, 7.25]);
addPl('tiny-fraction', cube, [1e-7, 1 - 1e-7]);
addPl('within-eps', cube, [4 - 1e-10, 2 + 1e-10]);
addPl('negative-off', cube, [-0.3, -0.3]);
for (let i = 0; i < 200; i++) {
  const xy = i % 3 === 0 ? [Math.round(between(-90, 90)), Math.round(between(-90, 90))] : [between(-90, 90), between(-90, 90)];
  addPl(`rand${i}`, grids[i % 40], xy);
}
writeCases('footprint', 'placed', pl);

// ---- shared
const sh = [];
const addSh = (id, a, b) => sh.push({ id, args: [clone(a), clone(b)], out: fp.shared(a, b) });
addSh('null-a', null, cube);
addSh('null-b', cube, null);
addSh('L-in-corner', fp.placed(L, [0, 0]), fp.placed(cube, [15, 15]));
addSh('L-overlap', fp.placed(L, [0, 0]), fp.placed(cube, [5, 5]));
addSh('apart', fp.placed(cube, [0, 0]), fp.placed(cube, [50, 0]));
for (let i = 0; i < 200; i++) {
  addSh(`rand${i}`, fp.placed(grids[i % 40], [Math.round(between(-20, 20)), Math.round(between(-20, 20))]),
    fp.placed(grids[(i * 5 + 1) % 40], [between(-20, 20), between(-20, 20)]));
}
writeCases('footprint', 'shared', sh);

// ---- dilate, area
const di = [], ar = [];
for (let i = 0; i < 60; i++) {
  const g = grids[i % 40], r = [-1, 0, 1, 2, 3, 10][i % 6];
  di.push({ id: `rand${i}`, args: [clone(g), r], out: fp.dilate(g, r) });
}
di.push({ id: 'box-3', args: [fp.rasterize(f32(box(0.2, 0.2, 0.8, 0.8))), 3], out: fp.dilate(fp.rasterize(f32(box(0.2, 0.2, 0.8, 0.8))), 3) });
writeCases('footprint', 'dilate', di);
ar.push({ id: 'null', args: [null], out: fp.area(null) });
for (let i = 0; i < 40; i++) ar.push({ id: `rand${i}`, args: [clone(grids[i])], out: fp.area(grids[i]) });
writeCases('footprint', 'area', ar);

// ---- bedGrid, take
const bg = [], tk = [];
for (const [bx, by] of [[250, 220], [180, 180], [256, 256], [100, 100], [33, 17], [120.5, 110.5], [1, 1]]) {
  bg.push({ id: `${bx}x${by}`, args: [bx, by], out: fp.bedGrid(bx, by) });
}
writeCases('footprint', 'bedGrid', bg);
for (let i = 0; i < 80; i++) {
  const bed = fp.bedGrid([120, 100, 90][i % 3], [110, 100, 80][i % 3]);
  const before = clone(bed);
  const g = fp.placed(grids[i % 40], [between(-70, 70), between(-60, 60)]);
  fp.take(bed, g);
  tk.push({ id: `rand${i}`, args: [before, clone(g)], out: bed });
}
tk.push({ id: 'null', args: [fp.bedGrid(100, 100), null], out: fp.bedGrid(100, 100) });
writeCases('footprint', 'take', tk);

// ---- findSpot, pack
const fs = [];
for (let i = 0; i < 120; i++) {
  const bx = [120, 100, 90, 60][i % 4], by = [110, 100, 80, 60][i % 4];
  const bed = fp.bedGrid(bx, by);
  const k = Math.floor(rnd() * 4);
  for (let j = 0; j < k; j++) fp.take(bed, fp.placed(grids[(i + j * 3) % 40], [Math.round(between(-bx / 3, bx / 3)), Math.round(between(-by / 3, by / 3))]));
  const g = grids[(i * 3 + 1) % 40], gap = [10, 0, 5, 2][i % 4], step = i % 5 === 4 ? 3 : 5;
  fs.push({ id: `rand${i}`, args: [clone(bed), clone(g), gap, step], out: fp.findSpot(bed, g, gap, step) });
}
{
  const bed = fp.bedGrid(120, 120), g = fp.rasterize(f32(box(-10, -10, 10, 10)));
  fp.take(bed, fp.placed(g, [0, 0]));
  fs.push({ id: 'beside-taken', args: [clone(bed), clone(g), 10, 5], out: fp.findSpot(bed, g, 10, 5) });
}
writeCases('footprint', 'findSpot', fs);

const pk = [];
const addPk = (id, list, bx, by, gap) => pk.push({ id, args: [list.map(clone), bx, by, gap], out: fp.pack(list, bx, by, gap) });
addPk('five', [fp.rasterize(f32(box(-20, -20, 20, 20))), L, fp.rasterize(f32(box(-30, -5, 30, 5))),
  fp.rasterize(f32(box(-8, -8, 8, 8))), fp.rasterize(f32(box(-25, -12, 25, 12)))], 180, 180, 10);
{
  const g = fp.rasterize(f32(box(-40, -40, 40, 40)));
  addPk('no-room', [g, g, g], 100, 100, 10);
  addPk('too-big', [fp.rasterize(f32(box(0, 0, 120, 5)))], 100, 100, 10);
}
addPk('null-grid', [cube, null, cube], 100, 100, 10);
addPk('empty', [], 100, 100, 10);
for (let i = 0; i < 80; i++) {
  const n = 2 + Math.floor(rnd() * 5);
  addPk(`rand${i}`, Array.from({ length: n }, (_, j) => (rnd() < 0.05 ? null : grids[(i * 3 + j * 7) % 40])),
    [120, 100, 110, 90][i % 4], [110, 100, 120, 90][i % 4], [10, 5, 0][i % 3]);
}
writeCases('footprint', 'pack', pk);

// ---- dragStep
const ds = [];
const addDs = (id, at, to, me, others) => {
  const clash = (xy) => others.reduce((n, o) => n + fp.shared(fp.placed(me, xy), o).length, 0);
  ds.push({ id, args: [at, to, clone(me), others.map(clone)], out: fp.dragStep(at, to, clash) });
};
{
  const wall = fp.placed(cube, [20, 0]);
  addDs('clear', [0, 0], [5, -3], cube, [wall]);
  addDs('into', [10, 0], [15, 0], cube, [wall]);
  addDs('slide', [10, 0], [14, 4], cube, [wall]);
  addDs('clear-whole', [9, -12], [12, -11], cube, [wall]);
  addDs('deeper', [15, 0], [18, 0], cube, [wall]);
  addDs('out', [15, 0], [12, 0], cube, [wall]);
  addDs('fast', [0, 0], [15, 0], cube, [wall]);
  addDs('jump', [0, 0], [40, 0], cube, [wall]);
  addDs('stay', [3, 3], [3, 3], cube, [wall]);
}
for (let i = 0; i < 200; i++) {
  const me = grids[i % 40];
  const others = Array.from({ length: 1 + (i % 3) }, (_, j) => fp.placed(grids[(i * 5 + j * 9 + 2) % 40], [Math.round(between(-40, 40)), Math.round(between(-40, 40))]));
  const at = [Math.round(between(-60, 60) * 10) / 10, Math.round(between(-60, 60) * 10) / 10];
  const to = [Math.round((at[0] + between(-25, 25)) * 10) / 10, Math.round((at[1] + between(-25, 25)) * 10) / 10];
  addDs(`rand${i}`, at, to, me, others);
}
writeCases('footprint', 'dragStep', ds);

// ---- plateUnder
const pu = [];
const addPu = (id, ...a) => pu.push({ id, args: a, out: plateUnder(...a) });
addPu('next', [300.5, -40], 1, 3, 280, 250, 220);
addPu('prev', [-280, 0], 1, 3, 280, 250, 220);
addPu('own', [100, 0], 1, 3, 280, 250, 220);
addPu('gap', [140, 0], 1, 3, 280, 250, 220);
addPu('far-side', [290, 115], 1, 3, 280, 250, 220);
addPu('past', [560, 0], 1, 3, 280, 250, 220);
addPu('before-first', [-600, 0], 1, 3, 280, 250, 220);
addPu('exact-edge', [280 + 125, 110], 0, 3, 280, 250, 220);
addPu('negative-zero', [-0, 0], 0, 2, 280, 250, 220);
for (let i = 0; i < 200; i++) {
  const count = 1 + (i % 5), active = Math.floor(rnd() * count);
  addPu(`rand${i}`, [between(-900, 900), between(-150, 150)], active, count, [280, 300, 220][i % 3], [250, 256, 180][i % 3], [220, 256, 180][i % 3]);
}
writeCases('footprint', 'plateUnder', pu);
