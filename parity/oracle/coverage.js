// Records parity/corpus/coverage/*.jsonl from web/fins/coverage.js (direct, *.soup for synthetic parts,
// *.captured from the golden scenes).
//   reachOf               args: [tops, R, queries, adds]   out: {before: [bool], after: [bool]}  (the closure is
//                         recorded by applying it: every query, then `near.add` for each of `adds`, then every query)
//   classify              args: [topo, analysis, rot]       out: Uint8Array of classes (MUST 0, HOLE 1, LOW 2)
//   classify.soup         args: [pos, thresholdDeg, rot]    out: as above
//   automaticTargets      args: [topo, analysis, rot]       out: the augmented copy of the analyze result
//   automaticTargets.soup args: [pos, thresholdDeg, rot]    out: as above
//   touchesSmallHole      args: [result, line]              out: boolean. Direct cases pass `{excludedSupportTris}`
//                         only (the one key it reads); captured ones pass the whole augmented result.
// The analyze result is cloned before it is recorded: its over/kept/onBed are the topology's scratch arrays.
import { writeCases } from "./lib/encode.js";
import { analysisRef, analyze, buildTopology, cfgRef, modelList, sitePose, topoOf, topoRef } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { UPSTREAM_ROOT, WEB } from "./lib/pin.js";

const { reachOf, classify, automaticTargets, touchesSmallHole } = await import(`${WEB}fins/coverage.js`);
const { circularBore } = await import(`file://${UPSTREAM_ROOT}/tests/_bore.js`);
const { recessedBody } = await import(`file://${UPSTREAM_ROOT}/tests/_circular.js`);

const cases = { reachOf: [], classify: [], automaticTargets: [], touchesSmallHole: [] };
const soup = { classify: [], automaticTargets: [], touchesSmallHole: [] };
const clone = (x) => structuredClone(x);

// --- reachOf: a deterministic cloud of wall tops, queries around and between them ---
{
  const tops = [];
  for (let i = 0; i < 12; i++) for (let j = 0; j < 7; j++) tops.push([i * 3.7 - 5, j * 2.9 - 4, 2 + ((i * 7 + j * 3) % 11) * 0.4]);
  const queries = [];
  for (let i = 0; i < 9; i++) for (let j = 0; j < 6; j++) for (const dz of [-0.2, 0.3, 1.2, 1.7, 4]) queries.push([i * 4.1 - 6, j * 3.3 - 5, 2.5 + dz + (i % 3) * 0.5]);
  for (const R of [0.5, 1, 3, 6, 12]) {
    const adds = [[0, 0, 3], [10.2, 4.4, 5], [-7, -6, 1], [20, 20, 2]];
    const near = reachOf(tops, R);
    const before = queries.map((q) => near(...q));
    for (const p of adds) near.add(p);
    const after = queries.map((q) => near(...q));
    cases.reachOf.push({ id: `R${R}`, args: [tops, R, queries, adds], out: { before, after } });
  }
  const empty = reachOf([], 2);
  cases.reachOf.push({ id: "empty", args: [[], 2, queries.slice(0, 5), [[1, 1, 1]]], out: (() => { const b = queries.slice(0, 5).map((q) => empty(...q)); empty.add([1, 1, 1]); return { before: b, after: queries.slice(0, 5).map((q) => empty(...q)) }; })() });
}

// --- the models ---
const POSES = [[0, 0, 0], [90, 0, 0], [0, 90, 0], [40, 0, 0], [-45, 30, 15], [-90, 0, 0]];
let holes = 0, low = 0;
for (const [name] of modelList()) {
  const topo = topoOf(name);
  for (const deg of POSES) {
    const rot = sitePose(deg);
    const res = analyze(topo, 45, rot);
    const id = `${name}|r${deg.join(",")}`;
    const args = [topoRef(name), analysisRef(name, 45, rot), rot];
    const cls = classify(topo, res, rot);
    if (cls.includes(1)) holes++;
    if (cls.includes(2)) low++;
    cases.classify.push({ id, args, out: cls });
    cases.automaticTargets.push({ id, args, out: clone(automaticTargets(topo, res, rot)) });
  }
}

// --- synthetic parts: bores (the tiny-hole route), recessed bodies, collars, a roof without a floor ---
const geometry = (pos) => ({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });
const quad = (a, b, c, d) => [a, b, c, a, c, d];
function block(x0, x1, y0, y1, z0, z1) {
  const v = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
  return [...quad(v[0], v[3], v[2], v[1]), ...quad(v[4], v[5], v[6], v[7]), ...quad(v[0], v[1], v[5], v[4]),
    ...quad(v[2], v[3], v[7], v[6]), ...quad(v[1], v[2], v[6], v[5]), ...quad(v[0], v[4], v[7], v[3])];
}
const flat = (tris) => Float64Array.from(tris.flat());
const parts = {
  "bore-2": circularBore({ diameter: 2 }),
  "bore-3": circularBore({ diameter: 3 }),
  "bore-5": circularBore({ diameter: 5 }),
  "bore-6": circularBore({ diameter: 6 }),
  "bore-7": circularBore({ diameter: 7 }),
  "bore-10": circularBore({ diameter: 10 }),
  "bore-3-ax3": circularBore({ diameter: 3, axialSegments: 3 }),
  "bore-5-seg24": circularBore({ diameter: 5, segments: 24 }),
  "bore-and-block": Float64Array.from([...circularBore(), ...flat(block(30, 60, -10, 10, 10, 15))]),
  recessed: recessedBody(),
  "recessed-small": recessedBody({ opening: 6, bore: 2.5, depth: 5, outer: 12, height: 20 }),
  "collar-on-post": flat([...block(-1, 1, -1, 1, 0, 20), ...block(-5, 5, -5, 5, 10, 11)]),
  "ledge-on-post": flat([...block(-5, 5, -5, 5, 0, 20), ...block(-5, 25, -8, 8, 20, 24)]),
  "near-plate-lip": flat([...block(-10, 10, -10, 10, 0.7, 5), ...block(-12, -10, -10, 10, 0, 5)]),
  "slot-ceiling": flat([...block(-15, 15, -10, 10, 0, 6), ...block(-15, 15, -10, -1, 6, 12), ...block(-15, 15, 1, 10, 6, 12), ...block(-15, 15, -1, 1, 7, 12)]),
};
for (const [name, pos] of Object.entries(parts)) {
  const topo = buildTopology(geometry(pos));
  for (const [thr, deg] of [[45, [0, 0, 0]], [70, [0, 0, 0]], [45, [30, 0, 0]], [45, [0, 90, 0]]]) {
    const rot = sitePose(deg);
    const res = analyze(topo, thr, rot);
    const id = `${name}|t${thr}|r${deg.join(",")}`;
    const cls = classify(topo, res, rot);
    if (cls.includes(1)) holes++;
    if (cls.includes(2)) low++;
    soup.classify.push({ id, args: [pos, thr, rot], out: cls });
    const t = automaticTargets(topo, res, rot);
    soup.automaticTargets.push({ id, args: [pos, thr, rot], out: clone(t) });
    // lines to test against the excluded surfaces: through the ceilings, near them, far away
    if (t.excludedSupportTris.length) {
      const tr = t.excludedSupportTris;
      const lines = [];
      for (let f = 0; f * 9 < tr.length && lines.length < 6; f += 3) {
        const x = (tr[f * 9] + tr[f * 9 + 3] + tr[f * 9 + 6]) / 3, y = (tr[f * 9 + 1] + tr[f * 9 + 4] + tr[f * 9 + 7]) / 3, z = (tr[f * 9 + 2] + tr[f * 9 + 5] + tr[f * 9 + 8]) / 3;
        lines.push([[x, y, z], [x + 1, y, z + 0.2]], [[x, y, z + 0.4], [x, y + 1, z + 0.45]], [[x, y, z + 0.9]], [[x + 40, y, z]]);
      }
      lines.forEach((line, k) => soup.touchesSmallHole.push({ id: `${id}|l${k}`, args: [{ excludedSupportTris: tr }, line], out: touchesSmallHole(t, line) }));
    }
  }
}
// no excluded surface at all (undefined and empty)
cases.touchesSmallHole.push({ id: "empty", args: [{ excludedSupportTris: [] }, [[1, 2, 3]]], out: touchesSmallHole({ excludedSupportTris: [] }, [[1, 2, 3]]) });
cases.touchesSmallHole.push({ id: "none", args: [{}, [[1, 2, 3]]], out: touchesSmallHole({}, [[1, 2, 3]]) });
cases.touchesSmallHole.push({ id: "empty-line", args: [{ excludedSupportTris: [0, 0, 0, 1, 0, 0, 0, 1, 0] }, []], out: false });

for (const [fn, list] of Object.entries(cases)) writeCases("coverage", fn, list);
for (const [fn, list] of Object.entries(soup)) writeCases("coverage", `${fn}.soup`, list);

await recordGolden("coverage", [{ rel: "fins/coverage.js", fns: ["classify", "automaticTargets", "touchesSmallHole"] }]);
// no scene reaches touchesSmallHole from outside coverage.js: no empty file
for (const f of ["touchesSmallHole.captured.jsonl"]) {
  const path = new URL(`../corpus/coverage/${f}`, import.meta.url);
  try { if (Deno.statSync(path).size === 0) Deno.removeSync(path); } catch { /* absent */ }
}
console.log("coverage:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])), `models/soups with HOLE ${holes}, LOW ${low}`);
