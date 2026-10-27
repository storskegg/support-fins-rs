// Records parity/corpus/bridge/flatRunAxis.jsonl (direct) and flatRunAxis.captured.jsonl (golden scenes)
// from web/prop/bridge.js.
//   flatRunAxis  args: [pts, patchTris, step, support, {$config: {PROP}}]
//                support: null, or {topo: {$topo}, rot, offset} (the part the anchors are read from)
//                out (direct): {axis: [ux, uy], turned, old}   (the JS array's `turned` / `old` properties;
//                              `turned` is false and `old` is undefined for the plain old-rule returns)
//                out (captured): [ux, uy]  (the capture encodes the bare array, so `turned`/`old` are
//                              checked only by the direct cases)
import { writeCases } from "./lib/encode.js";
import { analyze, buildTopology, cfgRef, REGION_MODELS, REGION_POSES, regionsOf, sitePose, topoRef } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const { flatRunAxis } = await import(`${WEB}prop/bridge.js`);

const asObj = (a) => ({ axis: [a[0], a[1]], turned: !!a.turned, old: a.old });
const cases = [];
const STEPS = [1, 2.5, 0.4];
for (const name of REGION_MODELS) {
  for (const deg of REGION_POSES) {
    for (const r of regionsOf(name, deg, 3)) {
      const tag = `${name}|r${deg.join(",")}|g${r.ri}`;
      const sup = { topo: topoRef(name), rot: r.rot, offset: r.offset };
      const realSup = { topo: r.topo, rot: r.rot, offset: r.offset };
      for (const step of STEPS) {
        cases.push({ id: `${tag}|s${step}|sup`, args: [r.pts, r.tris, step, sup, cfgRef("PROP")], out: asObj(flatRunAxis(r.pts, r.tris, step, realSup)) });
      }
      cases.push({ id: `${tag}|nosup`, args: [r.pts, r.tris, 1, null, cfgRef("PROP")], out: asObj(flatRunAxis(r.pts, r.tris, 1, null)) });
    }
  }
}
// degenerate footprints: a point cloud with no extent in x, or in y (the old rule, no support read)
{
  const { tris } = regionsOf("cube", [40, 0, 0], 1)[0];
  for (const [tag, pts] of Object.entries({ line_x: [[0, 0, 1], [5, 0, 1]], line_y: [[0, 0, 1], [0, 5, 1]], one: [[2, 2, 1]], wide: [[0, 0, 1], [9, 3, 1]], tall: [[0, 0, 1], [3, 9, 1]] })) {
    cases.push({ id: `edge-${tag}`, args: [pts, tris, 1, null, cfgRef("PROP")], out: asObj(flatRunAxis(pts, tris, 1, null)) });
  }
}
// --- synthetic parts: a ledge on one post, on two posts (a bridge), a cantilever, a plate over a gap ---
const quad = (a, b, c, d) => [a, b, c, a, c, d];
function block(x0, x1, y0, y1, z0, z1) {
  const v = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
  return [...quad(v[0], v[3], v[2], v[1]), ...quad(v[4], v[5], v[6], v[7]), ...quad(v[0], v[1], v[5], v[4]),
    ...quad(v[2], v[3], v[7], v[6]), ...quad(v[1], v[2], v[6], v[5]), ...quad(v[0], v[4], v[7], v[3])];
}
const parts = {
  "ledge-one-post": [...block(-5, 5, -5, 5, 0, 20), ...block(-5, 25, -8, 8, 20, 24)],
  "ledge-two-posts": [...block(-5, 5, -5, 5, 0, 20), ...block(25, 35, -5, 5, 0, 20), ...block(-5, 35, -8, 8, 20, 24)],
  "bridge-wide": [...block(-5, 5, -15, 15, 0, 20), ...block(25, 35, -15, 15, 0, 20), ...block(-5, 35, -15, 15, 20, 23)],
  "cantilever": [...block(-5, 5, -8, 8, 0, 20), ...block(-5, 40, -8, 8, 20, 22)],
  "deck-split-by-post": [...block(-3, 3, -3, 3, 0, 20), ...block(-25, 25, -10, 10, 20, 23)],
  "narrow-ledge": [...block(-5, 5, -2, 2, 0, 20), ...block(-5, 30, -2, 2, 20, 22)],
  "side-post": [...block(10, 14, -10, 10, 0, 22), ...block(-12, 12, -10, 10, 20, 22)],
};
const soupCases = [];
for (const [name, tris] of Object.entries(parts)) {
  const pos = Float32Array.from(tris.flat());
  const topo = buildTopology({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });
  const rot = sitePose([0, 0, 0]);
  const res = analyze(topo, 45, rot);
  res.regions.slice(0, 2).forEach((region, gi) => {
    const rtris = new Float64Array(region.faces.length * 9);
    const pts = [];
    region.faces.forEach((f, k) => {
      let gx = 0, gy = 0, gz = 0;
      for (let q = 0; q < 3; q++) {
        const o = f * 9 + q * 3;
        const x = topo.pos[o], y = topo.pos[o + 1], z = topo.pos[o + 2];
        const sx = rot[0] * x + rot[3] * y + rot[6] * z + res.offset.x;
        const sy = rot[1] * x + rot[4] * y + rot[7] * z + res.offset.y;
        const sz = rot[2] * x + rot[5] * y + rot[8] * z + res.offset.z;
        rtris[k * 9 + q * 3] = sx; rtris[k * 9 + q * 3 + 1] = sy; rtris[k * 9 + q * 3 + 2] = sz;
        pts.push([sx, sy, sz]); gx += sx; gy += sy; gz += sz;
      }
      pts.push([gx / 3, gy / 3, gz / 3]);
    });
    for (const step of [0.5, 1, 2]) {
      const support = { topo, rot, offset: res.offset };
      soupCases.push({ id: `${name}|g${gi}|s${step}`, args: [pos, 45, rot, gi, step], out: asObj(flatRunAxis(pts, rtris, step, support)) });
    }
  });
}
writeCases("bridge", "flatRunAxis.soup", soupCases);
writeCases("bridge", "flatRunAxis", cases);

await recordGolden("bridge", [{ rel: "prop/bridge.js", fns: ["flatRunAxis"] }], {
  config: { flatRunAxis: ["PROP"] },
  configs: async (cap) => {
    const m = await import(new URL("web/prop/config.js", cap.rootUrl).href);
    return { PROP: () => m.PROP };
  },
});
const turned = cases.filter((c) => c.out.turned).length;
console.log(`bridge: ${cases.length} direct (${turned} turned), ${soupCases.length} soups (${soupCases.filter((c) => c.out.turned).length} turned)`);
