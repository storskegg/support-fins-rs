// Records parity/corpus/judge/*.jsonl (direct) and the .captured.jsonl of the functions whose results are data
// (web/prop/judge.js).
// partAnchors   args: [$topo, $analysis, rot]                                  out: [[x,y,z]...]
// faceSamples   args: [$topo, $analysis, rot, f, {$config: {JUDGE}}]           out: [[x,y,z,w]...]
// scorer        args: [tops (lines), partPts, points, {$config: {PROP, JUDGE}}] out: ['near'|'bridged'|'cant'|'bare'...]
//               (the JS returns a function: the case applies it to each of `points`; direct only)
// judgeRuler    args: [$topo, $analysis, rot, faces, tops, {$config: {PROP, JUDGE}}]  out: held area
//               (the JS returns a function: the case applies it to `faces`, `tops`; direct only)
//               faces are [cx, cy, cz, area, region, face]; tops the flattened wall points
// attached      args: [$topo, $analysis, rot, o, n, shared, p, q]              out: boolean
// sharedEdge    args: [pos, o, n]  (pos: Float32Array of the two faces)        out: [a, b] | null
// otherVertex   args: [pos, n, shared]                                         out: 0 | 1 | 2
import { Ref, encode, writeCases } from "./lib/encode.js";
import { analysisRef, cfgRef, modelList, posed, REGION_MODELS, REGION_POSES, regionsOf, topoRef } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const { partAnchors, faceSamples, scorer, judgeRuler, attached, sharedEdge, otherVertex } = await import(`${WEB}prop/judge.js`);
const { contactLine } = await import(`${WEB}prop/contact.js`);

const cases = { partAnchors: [], faceSamples: [], scorer: [], judgeRuler: [], attached: [], sharedEdge: [], otherVertex: [] };
const POSES = [[40, 0, 0], [0, 35, 20], [90, 0, 0], [0, 0, 0], [-45, 30, 15]];

for (const [name] of modelList()) {
  for (const deg of POSES) {
    const p = posed(name, deg);
    const { topo, rot, res, offset } = p;
    if (!res.regions.length) continue;
    const tag = `${name}|r${deg.join(",")}`;
    const refs = [topoRef(name), analysisRef(name, 45, rot), rot];
    cases.partAnchors.push({ id: tag, args: refs, out: partAnchors(topo, res, rot) });
    // face samples for the first faces of each region
    for (const [gi, region] of res.regions.slice(0, 3).entries()) {
      for (const f of region.faces.slice(0, 4)) {
        cases.faceSamples.push({ id: `${tag}|g${gi}f${f}`, args: [...refs, f, cfgRef("JUDGE")], out: faceSamples(topo, res, rot, f) });
      }
    }
    // attached / sharedEdge / otherVertex over the overhang boundary edges
    let nAtt = 0;
    for (let k = 0; k < topo.adjA.length && nAtt < 30; k++) {
      const a = topo.adjA[k], b = topo.adjB[k];
      if (!res.over[a] === !res.over[b]) continue;
      const o = res.over[a] ? a : b, n = res.over[a] ? b : a;
      const shared = sharedEdge(topo.pos, o, n);
      if (!shared) continue;
      const S = (q) => {
        const x = topo.pos[q], y = topo.pos[q + 1], z = topo.pos[q + 2];
        return [rot[0] * x + rot[3] * y + rot[6] * z + offset.x, rot[1] * x + rot[4] * y + rot[7] * z + offset.y, rot[2] * x + rot[5] * y + rot[8] * z + offset.z];
      };
      const [pp, qq] = shared.map(S);
      nAtt++;
      cases.attached.push({ id: `${tag}|k${k}`, args: [...refs, o, n, shared, pp, qq], out: attached(topo, res, rot, o, n, shared, pp, qq) });
      const pair = new Float32Array(18);
      pair.set(topo.pos.subarray(o * 9, o * 9 + 9), 0);
      pair.set(topo.pos.subarray(n * 9, n * 9 + 9), 9);
      const sh = sharedEdge(pair, 0, 1);
      cases.sharedEdge.push({ id: `${tag}|k${k}`, args: [pair, 0, 1], out: sh });
      if (sh) cases.otherVertex.push({ id: `${tag}|k${k}`, args: [pair, 1, sh], out: otherVertex(pair, 1, sh) });
    }
  }
}
// sharedEdge: no shared edge, one shared vertex, identical faces
{
  const f = (a) => new Float32Array(a);
  const tri = [0, 0, 0, 1, 0, 0, 0, 1, 0];
  for (const [tag, other] of Object.entries({ none: [5, 5, 5, 6, 5, 5, 5, 6, 5], vertex: [0, 0, 0, 5, 5, 5, 6, 5, 5], same: tri, swapped: [1, 0, 0, 0, 0, 0, 0, -1, 0] })) {
    const pair = f([...tri, ...other]);
    const sh = sharedEdge(pair, 0, 1);
    cases.sharedEdge.push({ id: `synthetic-${tag}`, args: [pair, 0, 1], out: sh });
    cases.otherVertex.push({ id: `synthetic-${tag}`, args: [pair, 1, sh ?? [0, 3]], out: otherVertex(pair, 1, sh ?? [0, 3]) });
  }
}

// scorer and judgeRuler on the biggest regions' contact lines
for (const name of REGION_MODELS) {
  for (const deg of REGION_POSES) {
    for (const r of regionsOf(name, deg, 2)) {
      const line = contactLine(r.pts, r.soup, 12);
      if (!line) continue;
      const tag = `${name}|r${deg.join(",")}|g${r.ri}`;
      // wall tops 0.3 under the line (so some of the overhang is near them)
      const top = line.map((q) => [q[0], q[1], q[2] - 0.3]);
      const samples = r.region.faces.slice(0, 12).flatMap((f) => faceSamples(r.topo, r.res, r.rot, f));
      const pts = samples.map((s) => [s[0], s[1], s[2]]);
      // far and near probes too
      pts.push([top[0][0] + 100, top[0][1], top[0][2]], [top[0][0], top[0][1], top[0][2] + 0.2], [top[0][0] + 5, top[0][1] + 1, top[0][2]]);
      const anchors = partAnchors(r.topo, r.res, r.rot);
      for (const [vtag, tops] of Object.entries({ line: [top], none: [], two: [top.slice(0, 3), top.slice(-3)] })) {
        const s = scorer(tops, anchors);
        cases.scorer.push({ id: `${tag}|${vtag}`, args: [tops, anchors, pts, cfgRef("PROP", "JUDGE")], out: pts.map((q) => s(...q)) });
      }
      const faces = r.region.faces.slice(0, 20).map((f) => [0, 0, 0, r.topo.area[f], r.ri, f]);
      const ruler = judgeRuler(r.topo, r.res, r.rot);
      const flat = top.concat(top.slice(0, 2));
      cases.judgeRuler.push({ id: `${tag}|walls`, args: [topoRef(name), analysisRef(name, 45, r.rot), r.rot, faces, flat, cfgRef("PROP", "JUDGE")], out: ruler(faces, flat) });
      cases.judgeRuler.push({ id: `${tag}|bare`, args: [topoRef(name), analysisRef(name, 45, r.rot), r.rot, faces, [], cfgRef("PROP", "JUDGE")], out: ruler(faces, []) });
    }
  }
}
for (const [fn, list] of Object.entries(cases)) writeCases("judge", fn, list);

// captured: the functions that return data (scorer / judgeRuler return closures)
await recordGolden("judge", [{ rel: "prop/judge.js", fns: ["partAnchors", "faceSamples", "attached"] }], {
  config: { faceSamples: ["JUDGE"] },
  configs: async (cap) => {
    const m = await import(new URL("web/prop/judge.js", cap.rootUrl).href);
    return { JUDGE: () => m.JUDGE };
  },
});
console.log("judge:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])));
