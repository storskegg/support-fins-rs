// Records parity/corpus/tracks/{splitRegion,tubeLine,patchTracks}.jsonl (direct), *.soup.jsonl
// (synthetic parts) and *.captured.jsonl (golden scenes) from web/prop/tracks.js. All read PROP, so
// each call's args end with {"$config": {PROP}}.
//   splitRegion  args: [topo, faces, rot]                                out: [{faces, area}]
//   tubeLine     args: [topo, faces, rot, pts, regionTris, step?]        out: [[[x,y,z]...]...] | null
//   patchTracks  args: [pts, patchTris, step?, support?, span?, axis?, turn?]
//                support: null or {topo: {$topo}, rot, offset, sway}
//                out (direct): {lines, spacing, turned}  (the JS array's `spacing` / `turned` properties;
//                              `spacing` is undefined and `lines` empty on the early returns)
//                out (captured): the bare array of tracks (the capture cannot carry the properties)
//   *.soup       args: [pos (Float32Array), thresholdDeg, rot, ...] of a synthetic part; the replay
//                builds the topology and analysis itself and uses region 0 (as regionsOf does).
import { encode, writeCases } from "./lib/encode.js";
import { analyze, buildTopology, cfgMods, cfgRef, REGION_MODELS, REGION_POSES, regionsOf, sitePose, topoRef, withConfig } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const { splitRegion, tubeLine, patchTracks } = await import(`${WEB}prop/tracks.js`);
const PROP = cfgMods.PROP;

const cases = { splitRegion: [], tubeLine: [], patchTracks: [] };
const soup = { splitRegion: [], tubeLine: [], patchTracks: [] };
const asTracks = (t) => ({ lines: Array.from(t, (l) => l), spacing: t.spacing, turned: t.turned });

for (const name of REGION_MODELS) {
  for (const deg of REGION_POSES) {
    for (const r of regionsOf(name, deg, 4)) {
      const tag = `${name}|r${deg.join(",")}|g${r.ri}`;
      const faces = r.region.faces;
      cases.splitRegion.push({ id: tag, args: [topoRef(name), faces, r.rot, cfgRef("PROP")], out: splitRegion(r.topo, faces, r.rot) });
      for (const step of [undefined, 1.5]) {
        const a = [topoRef(name), faces, r.rot, r.pts, r.tris];
        const real = [r.topo, faces, r.rot, r.pts, r.tris];
        if (step !== undefined) { a.push(step); real.push(step); }
        cases.tubeLine.push({ id: `${tag}|s${step}`, args: [...a, cfgRef("PROP")], out: tubeLine(...real) });
      }
      const sup = (sway) => ({ topo: topoRef(name), rot: r.rot, offset: r.offset, sway });
      const real = (sway) => ({ topo: r.topo, rot: r.rot, offset: r.offset, sway });
      const variants = {
        defaults: [[r.pts, r.tris], [r.pts, r.tris]],
        sup: [[r.pts, r.tris, 1, sup(false), 12], [r.pts, r.tris, 1, real(false), 12]],
        sway: [[r.pts, r.tris, 1.5, sup(true), 8], [r.pts, r.tris, 1.5, real(true), 8]],
        sparse: [[r.pts, r.tris, 2, sup(false), 40], [r.pts, r.tris, 2, real(false), 40]],
        dense: [[r.pts, r.tris, 0.75, sup(false), 4], [r.pts, r.tris, 0.75, real(false), 4]],
        axisX: [[r.pts, r.tris, 1, null, 12, [1, 0]], [r.pts, r.tris, 1, null, 12, [1, 0]]],
        axisY: [[r.pts, r.tris, 1, sup(false), 12, [0, 1]], [r.pts, r.tris, 1, real(false), 12, [0, 1]]],
        noturn: [[r.pts, r.tris, 1, sup(false), 12, null, false], [r.pts, r.tris, 1, real(false), 12, null, false]],
      };
      for (const [vtag, [shown, run]] of Object.entries(variants)) {
        cases.patchTracks.push({ id: `${tag}|${vtag}`, args: [...shown, cfgRef("PROP")], out: asTracks(patchTracks(...run)) });
      }
    }
  }
}
// empty and one-point inputs
{
  const { tris } = regionsOf("cube", [40, 0, 0], 1)[0];
  for (const [tag, pts] of Object.entries({ empty: [], one: [[1, 1, 1]], same: [[1, 1, 1], [1, 1, 1]] })) {
    cases.patchTracks.push({ id: `edge-${tag}`, args: [pts, tris, 1, null, 12, null, true, cfgRef("PROP")], out: asTracks(patchTracks(pts, tris, 1, null, 12, null, true)) });
  }
}

// --- synthetic parts: pipes (the tube route, small and large), a bowl-like pocket, a stepped ledge ---
const quad = (a, b, c, d) => [a, b, c, a, c, d];
function pipe(len, r, n, x0, y0, z0) {
  // a closed cylinder lying along x, n facets round, outward winding
  const ring = (x) => Array.from({ length: n }, (_, i) => [x, y0 + r * Math.cos((2 * Math.PI * i) / n), z0 + r * Math.sin((2 * Math.PI * i) / n)]);
  const a = ring(x0), b = ring(x0 + len), t = [];
  for (let i = 0; i < n; i++) {
    const j = (i + 1) % n;
    t.push(...quad(a[i], b[i], b[j], a[j]));
    t.push(a[i], a[j], [x0, y0, z0]);
    t.push(b[j], b[i], [x0 + len, y0, z0]);
  }
  return t;
}
function block(x0, x1, y0, y1, z0, z1) {
  const v = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
  return [...quad(v[0], v[3], v[2], v[1]), ...quad(v[4], v[5], v[6], v[7]), ...quad(v[0], v[1], v[5], v[4]),
    ...quad(v[2], v[3], v[7], v[6]), ...quad(v[1], v[2], v[6], v[5]), ...quad(v[0], v[4], v[7], v[3])];
}
const f32 = (tris) => Float32Array.from(tris.flat());
const parts = {
  "pipe-small": [...block(-20, 20, -20, 20, 0, 1), ...pipe(14, 2, 12, -7, 0, 12)],
  "pipe-large": [...block(-40, 40, -20, 20, 0, 1), ...pipe(60, 10, 24, -30, 0, 20)],
  "pipe-on-post": [...block(-5, 5, -5, 5, 0, 20), ...pipe(30, 4, 16, -15, 0, 25)],
  "ledge-and-post": [...block(-5, 5, -5, 5, 0, 20), ...block(-5, 25, -8, 8, 20, 24)],
  "ledge-two-posts": [...block(-5, 5, -5, 5, 0, 20), ...block(25, 35, -5, 5, 0, 20), ...block(-5, 35, -8, 8, 20, 24)],
  "ledge-cantilever": [...block(-5, 5, -8, 8, 0, 20), ...block(-5, 40, -8, 8, 20, 22)],
  "floating-slab": [...block(-5, 5, -5, 5, 0, 5), ...block(-12, 12, -10, 10, 15, 17)],
};
function regionOf(topo, res, rot, i) {
  const region = res.regions[i];
  if (!region) return null;
  const tris = new Float64Array(region.faces.length * 9);
  const pts = [];
  region.faces.forEach((f, k) => {
    let gx = 0, gy = 0, gz = 0;
    for (let q = 0; q < 3; q++) {
      const o = f * 9 + q * 3, x = topo.pos[o], y = topo.pos[o + 1], z = topo.pos[o + 2];
      const sx = rot[0] * x + rot[3] * y + rot[6] * z + res.offset.x;
      const sy = rot[1] * x + rot[4] * y + rot[7] * z + res.offset.y;
      const sz = rot[2] * x + rot[5] * y + rot[8] * z + res.offset.z;
      tris[k * 9 + q * 3] = sx; tris[k * 9 + q * 3 + 1] = sy; tris[k * 9 + q * 3 + 2] = sz;
      pts.push([sx, sy, sz]);
      gx += sx; gy += sy; gz += sz;
    }
    pts.push([gx / 3, gy / 3, gz / 3]);
  });
  return { region, tris, pts };
}
let soupTube = 0, soupTurned = 0;
for (const [name, tris] of Object.entries(parts)) {
  const pos = f32(tris);
  const topo = buildTopology({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });
  for (const [thr, deg] of [[45, [0, 0, 0]], [45, [30, 0, 0]], [75, [0, 0, 0]], [75, [0, 30, 0]], [85, [0, 0, 0]]]) {
    const rot = sitePose(deg);
    const res = analyze(topo, thr, rot);
    for (let gi = 0; gi < Math.min(2, res.regions.length); gi++) {
      const g = regionOf(topo, res, rot, gi);
      const id = `${name}|t${thr}|r${deg.join(",")}|g${gi}`;
      soup.splitRegion.push({ id, args: [pos, thr, rot, gi], out: splitRegion(topo, g.region.faces, rot) });
      for (const step of [undefined, 1.5]) {
        const out = step === undefined ? tubeLine(topo, g.region.faces, rot, g.pts, g.tris) : tubeLine(topo, g.region.faces, rot, g.pts, g.tris, step);
        if (out) soupTube++;
        soup.tubeLine.push({ id: `${id}|s${step}`, args: [pos, thr, rot, gi, ...(step === undefined ? [] : [step])], out });
      }
      const support = { topo, rot, offset: res.offset, sway: false };
      for (const [vtag, extra] of Object.entries({ plain: [1, support, 12], nosup: [1, null, 12], span6: [1.5, support, 6] })) {
        const t = patchTracks(g.pts, g.tris, ...extra);
        if (t.turned) soupTurned++;
        soup.patchTracks.push({ id: `${id}|${vtag}`, args: [pos, thr, rot, gi, ...extra.slice(0, 1), extra[1] ? "support" : null, extra[2]], out: asTracks(t) });
      }
    }
  }
}
for (const [fn, list] of Object.entries(cases)) writeCases("tracks", fn, list);
for (const [fn, list] of Object.entries(soup)) writeCases("tracks", `${fn}.soup`, list);

await recordGolden("tracks", [{ rel: "prop/tracks.js", fns: ["splitRegion", "tubeLine", "patchTracks"] }], {
  config: { splitRegion: ["PROP"], tubeLine: ["PROP"], patchTracks: ["PROP"] },
  configs: async (cap) => {
    const m = await import(new URL("web/prop/config.js", cap.rootUrl).href);
    return { PROP: () => m.PROP };
  },
});
const tubes = cases.tubeLine.filter((c) => c.out).length;
const turned = cases.patchTracks.filter((c) => c.out.turned).length;
console.log("tracks:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])), `tube routes ${tubes}, turned ${turned}; soups tube ${soupTube} turned ${soupTurned}`);
