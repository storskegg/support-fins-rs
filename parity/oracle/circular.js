// Records parity/corpus/circular/{circularFootprint,circularRegions,circularTracks}*.jsonl from
// web/prop/circular.js (direct, *.soup for synthetic parts, *.captured from the golden scenes).
//   circularFootprint  args: [tris (Float64Array: a region's seated soup)]       out: frame | null
//   circularRegions    args: [topo, analysis, rot, {$config: {PROP}}]            out: Map(regionIndex -> frame)
//   circularRegions.soup  args: [pos (Float64Array), thresholdDeg, rot]          out: as above
//   circularTracks     args: [frame, tris, step, span, {$config: {PROP}}]        out: [[[x,y,z]...]...] | null
// A frame is {centre, innerRadius, outerRadius, radialProfile, angularExtent, phase, axis[, surrounding]}.
import { Ref, encode, writeCases } from "./lib/encode.js";
import { analysisRef, analyze, buildTopology, cfgMods, cfgRef, REGION_MODELS, REGION_POSES, regionsOf, sitePose, topoRef } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { UPSTREAM_ROOT, WEB } from "./lib/pin.js";

const { circularFootprint, circularRegions, circularTracks } = await import(`${WEB}prop/circular.js`);
const { circularBore } = await import(`file://${UPSTREAM_ROOT}/tests/_bore.js`);
const { recessedBody } = await import(`file://${UPSTREAM_ROOT}/tests/_circular.js`);

const cases = { circularFootprint: [], circularRegions: [], circularTracks: [] };
const soup = { circularFootprint: [], circularRegions: [], circularTracks: [] };

/** A region's seated soup, as buildProps passes it. */
function seatedRegion(topo, res, rot, region) {
  const tris = new Float64Array(region.faces.length * 9);
  let at = 0;
  for (const f of region.faces) {
    for (let i = 0; i < 9; i += 3) {
      const o = f * 9 + i, x = topo.pos[o], y = topo.pos[o + 1], z = topo.pos[o + 2];
      tris[at++] = rot[0] * x + rot[3] * y + rot[6] * z + res.offset.x;
      tris[at++] = rot[1] * x + rot[4] * y + rot[7] * z + res.offset.y;
      tris[at++] = rot[2] * x + rot[5] * y + rot[8] * z + res.offset.z;
    }
  }
  return tris;
}

// --- the stress models and fixtures (mostly "not circular": the null paths) ---
for (const name of REGION_MODELS) {
  for (const deg of REGION_POSES) {
    for (const r of regionsOf(name, deg, 4)) {
      const tris = seatedRegion(r.topo, r.res, r.rot, r.region);
      cases.circularFootprint.push({ id: `${name}|r${deg.join(",")}|g${r.ri}`, args: [tris], out: circularFootprint(tris) });
    }
    const p = regionsOf(name, deg, 1)[0];
    if (p) {
      cases.circularRegions.push({
        id: `${name}|r${deg.join(",")}`,
        args: [topoRef(name), analysisRef(name, 45, p.rot), p.rot, cfgRef("PROP")],
        out: circularRegions(p.topo, p.res, p.rot),
      });
    }
  }
}

// --- synthetic circular parts ---
const geometry = (pos) => ({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });
const parts = {
  recessed: recessedBody(),
  "recessed-48": recessedBody({ segments: 48 }),
  "recessed-yaw": recessedBody({ yaw: 3.7 }),
  "recessed-moved": recessedBody({ translate: [30, -12, 0] }),
  "recessed-shallow": recessedBody({ depth: 4, opening: 20, bore: 12 }),
  "recessed-flat": recessedBody({ depth: 0.005, opening: 25, bore: 15 }),
  "recessed-small": recessedBody({ opening: 6, bore: 2.5, depth: 5, outer: 12, height: 20 }),
  "bore-3": circularBore({ diameter: 3 }),
  "bore-5": circularBore({ diameter: 5 }),
  "bore-10": circularBore({ diameter: 10 }),
  "bore-ax2": circularBore({ diameter: 5, axialSegments: 2 }),
  "recess-and-bore": Float64Array.from([...recessedBody(), ...circularBore({ diameter: 4 }).map((v, i) => v + (i % 3 === 0 ? 100 : 0))]),
};
let frames = 0, tracks = 0;
for (const [name, pos] of Object.entries(parts)) {
  const topo = buildTopology(geometry(pos));
  for (const [thr, deg] of [[45, [0, 0, 0]], [70, [0, 0, 0]], [45, [20, 0, 0]], [45, [0, 0, 45]]]) {
    const rot = sitePose(deg);
    const res = analyze(topo, thr, rot);
    const id = `${name}|t${thr}|r${deg.join(",")}`;
    res.regions.forEach((region, gi) => {
      const tris = seatedRegion(topo, res, rot, region);
      soup.circularFootprint.push({ id: `${id}|g${gi}`, args: [tris], out: circularFootprint(tris) });
    });
    const map = circularRegions(topo, res, rot);
    soup.circularRegions.push({ id, args: [pos, thr, rot], out: map });
    for (const [ri, frame] of map) {
      frames++;
      const tris = seatedRegion(topo, res, rot, res.regions[ri]);
      for (const [step, span] of [[1, 12], [0.5, 6], [2, 30], [1, 3]]) {
        const out = circularTracks(frame, tris, step, span);
        if (out) tracks++;
        soup.circularTracks.push({ id: `${id}|g${ri}|s${step}|p${span}`, args: [frame, tris, step, span, cfgRef("PROP")], out });
      }
    }
  }
}
// hand-made inputs that must fall through to the old path
{
  const tri = (a, b, c) => [...a, ...b, ...c];
  const edge = [
    ["empty", []],
    ["degenerate", tri([0, 0, 0], [0, 0, 0], [1, 0, 0])],
    ["one-triangle", tri([0, 0, 0], [5, 0, 0], [0, 5, 0])],
    ["three-on-an-edge", [...tri([0, 0, 0], [1, 0, 0], [0, 1, 0]), ...tri([0, 0, 0], [1, 0, 0], [0, -1, 0]), ...tri([0, 0, 0], [1, 0, 0], [0.5, 0, 1])]],
  ];
  for (const [tag, t] of edge) {
    const tris = Float64Array.from(t);
    soup.circularFootprint.push({ id: `edge-${tag}`, args: [tris], out: circularFootprint(tris) });
  }
  // a fitted frame with the radial-profile guards: flat (no slope), too small, a thin ring
  const frame = (over) => ({ centre: [0, 0], innerRadius: 5, outerRadius: 15, radialProfile: [[5, 4], [15, 10]], angularExtent: [0, 2 * Math.PI], phase: 0.3, axis: [Math.cos(0.3), Math.sin(0.3)], ...over });
  const flatTris = Float64Array.from([-30, -30, 5, 30, -30, 5, 30, 30, 5, -30, -30, 5, 30, 30, 5, -30, 30, 5]);
  const slopeTris = Float64Array.from([-30, -30, 1, 30, -30, 1, 30, 30, 11, -30, -30, 1, 30, 30, 11, -30, 30, 11]);
  for (const [tag, f, t] of [
    ["flat-profile", frame({ radialProfile: [[5, 4], [15, 4.005]] }), flatTris],
    ["small-inner", frame({ innerRadius: 0.5, radialProfile: [[0.5, 4], [15, 10]] }), slopeTris],
    ["thin-ring", frame({ outerRadius: 5.5 }), slopeTris],
    ["no-cover", frame({}), Float64Array.from([100, 100, 1, 101, 100, 1, 100, 101, 1])],
    ["sloped", frame({}), slopeTris],
    ["dense-inner", frame({ innerRadius: 1.2, radialProfile: [[1.2, 3], [15, 10]] }), slopeTris],
  ]) {
    for (const [step, span] of [[1, 12], [0.4, 3]]) {
      soup.circularTracks.push({ id: `edge-${tag}|s${step}|p${span}`, args: [f, t, step, span, cfgRef("PROP")], out: circularTracks(f, t, step, span) });
    }
  }
}
for (const [fn, list] of Object.entries(cases)) if (list.length) writeCases("circular", fn, list);
for (const [fn, list] of Object.entries(soup)) writeCases("circular", `${fn}.soup`, list);

await recordGolden("circular", [{ rel: "prop/circular.js", fns: ["circularFootprint", "circularRegions", "circularTracks"] }], {
  config: { circularRegions: ["PROP"], circularTracks: ["PROP"] },
  configs: async (cap) => {
    const m = await import(new URL("web/prop/config.js", cap.rootUrl).href);
    return { PROP: () => m.PROP };
  },
});
// the golden scenes never reach circularTracks (no annular slope region): no empty file
for (const f of ["circularTracks.captured.jsonl", "circularTracks.jsonl"]) {
  const path = new URL(`../corpus/circular/${f}`, import.meta.url);
  try { if (Deno.statSync(path).size === 0) Deno.removeSync(path); } catch { /* absent */ }
}
console.log("circular:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])), `soup frames ${frames}, track sets ${tracks}`);
