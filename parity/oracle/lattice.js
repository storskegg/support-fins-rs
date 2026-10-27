// Records parity/corpus/lattice/{latticeStruts,strutPatches}*.jsonl from web/prop/lattice.js.
//   latticeStruts          args: [topo, faces, rot, offset]   out: {groups: [[face...]...], lost} | null
//                          (`seated` is a closure in JS; the replay rebuilds it from rot and offset with
//                          `seat`, as buildProps does: prop.js:363)
//   latticeStruts.soup     args: [pos (Float32Array), thresholdDeg, rot, regionIndex]   out: as above
//   strutPatches.soup      args: [pos, thresholdDeg, rot, regionIndex, step]
//                          out: the patches; each patch's `tris` (the whole region's soup, the same
//                          array for every patch) is recorded as the string "regionTris"
// The synthetic parts are welded nets: a plate on a grid of cells, with the solid cells forming bars.
// Captured calls (the golden scenes): `seated` cannot be encoded, so it is recorded as the seated
// triples of the faces involved, aligned with `faces` (latticeStruts) or with the groups flattened
// (strutPatches):  latticeStruts   args: [topo, faces, seatedTriples]   out: groups (the bare array; the
// capture drops its `lost` property)
//                 strutPatches    args: [topo, rot, groups, seatedTriples, step, region, regionTris]  out: patches
import { writeCases } from "./lib/encode.js";
import { analyze, buildTopology, cfgMods, REGION_MODELS, REGION_POSES, regionsOf, sitePose, topoRef } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const { latticeStruts, strutPatches } = await import(`${WEB}prop/lattice.js`);
const { seat } = await import(`${WEB}prop/surface.js`);

const seatedOf = (topo, rot, off) => (f) => [0, 1, 2].map((i) => seat(topo.pos, f * 9 + i * 3, rot, off, [0, 0, 0]));
const cases = [], soup = { latticeStruts: [], strutPatches: [] };
const asOut = (s) => (s ? { groups: Array.from(s, (g) => g), lost: s.lost } : null);

for (const name of REGION_MODELS) {
  for (const deg of REGION_POSES) {
    for (const r of regionsOf(name, deg, 4)) {
      const seated = seatedOf(r.topo, r.rot, r.offset);
      cases.push({ id: `${name}|r${deg.join(",")}|g${r.ri}`, args: [topoRef(name), r.region.faces, r.rot, r.offset], out: asOut(latticeStruts(r.topo, r.region.faces, seated)) });
    }
  }
}

// --- welded nets on a cell grid ---
const quad = (a, b, c, d) => [a, b, c, a, c, d];
function net({ n, cell = 1.5, solid, bottom = () => 20, top = 26, foot = true }) {
  const t = [];
  const P = (i, j, z) => [i * cell, j * cell, z];
  const b = (i, j) => P(i, j, bottom(i, j));
  const u = (i, j) => P(i, j, top);
  const on = (i, j) => i >= 0 && j >= 0 && i < n && j < n && solid(i, j);
  for (let j = 0; j < n; j++) for (let i = 0; i < n; i++) {
    if (!on(i, j)) continue;
    t.push(...quad(b(i, j), b(i, j + 1), b(i + 1, j + 1), b(i + 1, j)));           // underside, facing down
    t.push(...quad(u(i, j), u(i + 1, j), u(i + 1, j + 1), u(i, j + 1)));           // top
    if (!on(i - 1, j)) t.push(...quad(b(i, j), u(i, j), u(i, j + 1), b(i, j + 1)));
    if (!on(i + 1, j)) t.push(...quad(b(i + 1, j), b(i + 1, j + 1), u(i + 1, j + 1), u(i + 1, j)));
    if (!on(i, j - 1)) t.push(...quad(b(i, j), b(i + 1, j), u(i + 1, j), u(i, j)));
    if (!on(i, j + 1)) t.push(...quad(b(i + 1, j + 1), b(i, j + 1), u(i, j + 1), u(i + 1, j + 1)));
  }
  if (foot) { // a foot that sets the plate, clear of the net
    const x = -8, y = -8, z0 = 0, z1 = 22, w = 3;
    const v = [[x, y, z0], [x + w, y, z0], [x + w, y + w, z0], [x, y + w, z0], [x, y, z1], [x + w, y, z1], [x + w, y + w, z1], [x, y + w, z1]];
    t.push(...quad(v[0], v[3], v[2], v[1]), ...quad(v[4], v[5], v[6], v[7]), ...quad(v[0], v[1], v[5], v[4]),
      ...quad(v[2], v[3], v[7], v[6]), ...quad(v[1], v[2], v[6], v[5]), ...quad(v[0], v[4], v[7], v[3]));
  }
  return Float32Array.from(t.flat());
}
const bars = (w, gap) => (i, j) => i % gap < w || j % gap < w;
const dist2bar = (v, w, gap, cell) => { // distance (mm) from vertex coordinate v to the nearest bar centre line
  const c = (w * cell) / 2;
  const m = ((v * cell) % (gap * cell) + gap * cell) % (gap * cell);
  return Math.min(Math.abs(m - c), Math.abs(m - c - gap * cell));
};
const round = (w, gap, k, cell = 1.5) => (i, j) => 20 + k * Math.min(dist2bar(i, w, gap, cell), dist2bar(j, w, gap, cell)) ** 2;
const parts = {
  "net-grid": net({ n: 24, solid: bars(2, 6) }),
  "net-grid-wide": net({ n: 24, solid: bars(3, 7) }),
  "net-grid-coarse": net({ n: 30, cell: 2, solid: bars(2, 10) }),
  "net-diagonal": net({ n: 28, solid: (i, j) => Math.abs(i - j) % 9 < 2 || Math.abs(i + j - 27) % 9 < 2 }),
  "net-round": net({ n: 24, solid: bars(2, 6), bottom: round(2, 6, 0.14) }),
  "net-round-wide": net({ n: 24, solid: bars(4, 8), bottom: round(4, 8, 0.1), top: 30 }),
  "net-round-fine": net({ n: 60, cell: 0.5, solid: bars(6, 20), bottom: round(6, 20, 0.6, 0.5) }),
  "net-round-fine-small": net({ n: 40, cell: 0.5, solid: bars(5, 13), bottom: round(5, 13, 0.7, 0.5), top: 24 }),
  "net-holes": net({ n: 24, solid: (i, j) => (i % 6 < 4 || j % 6 < 4) && !(i % 6 === 5 && j % 6 === 5) }),
  "band": net({ n: 24, solid: (i, j) => j < 2 }),
  "plate": net({ n: 14, solid: () => true }),
  "ring": net({ n: 24, solid: (i, j) => { const r = Math.hypot(i - 11.5, j - 11.5); return r > 9 && r < 11; } }),
  "small-grid": net({ n: 10, solid: bars(2, 5) }),
  "huge-sparse": net({ n: 3, cell: 800, solid: () => true, foot: true }),
};
const geometry = (pos) => ({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });
let found = 0, tubePatches = 0, splits = 0;
for (const [name, pos] of Object.entries(parts)) {
  const topo = buildTopology(geometry(pos));
  for (const [thr, deg] of [[45, [0, 0, 0]], [45, [90, 0, 0]], [60, [0, 0, 30]]]) {
    const rot = sitePose(deg);
    const res = analyze(topo, thr, rot);
    res.regions.slice(0, 3).forEach((region, gi) => {
      const seated = seatedOf(topo, rot, res.offset);
      const s = latticeStruts(topo, region.faces, seated);
      const id = `${name}|t${thr}|r${deg.join(",")}|g${gi}`;
      soup.latticeStruts.push({ id, args: [pos, thr, rot, gi], out: asOut(s) });
      if (!s) return;
      found++;
      const regionTris = new Float64Array(region.faces.length * 9);
      region.faces.forEach((f, k) => seated(f).forEach((v, i) => regionTris.set(v, k * 9 + i * 3)));
      for (const step of [1, 0.5, 2]) {
        const patches = strutPatches(topo, rot, s, seated, step, gi, regionTris);
        for (const p of patches) { if (p.lines) tubePatches++; if (p.smallTube) splits++; }
        soup.strutPatches.push({ id: `${id}|s${step}`, args: [pos, thr, rot, gi, step], out: patches.map((p) => ({ ...p, tris: "regionTris" })) });
      }
    });
  }
}
if (!found) throw new Error("no synthetic net was recognised: the generator is wrong");
writeCases("lattice", "latticeStruts", cases);
writeCases("lattice", "latticeStruts.soup", soup.latticeStruts);
writeCases("lattice", "strutPatches.soup", soup.strutPatches);
console.log(`lattice: ${cases.length} model regions, ${soup.latticeStruts.length} soup regions (${found} nets), ${soup.strutPatches.length} strutPatches calls; tube patches ${tubePatches}, split races ${splits}`);

await recordGolden("lattice", [{ rel: "prop/lattice.js", fns: ["latticeStruts", "strutPatches"] }], {
  cap: 40,
  args: {
    latticeStruts: ([topo, faces, seated]) => [topo, faces, faces.map((f) => seated(f))],
    strutPatches: ([topo, rot, groups, seated, step, region, regionTris]) => [topo, rot, groups, groups.flat().map((f) => seated(f)), step, region, regionTris],
  },
});
