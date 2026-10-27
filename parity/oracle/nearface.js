// Records parity/corpus/nearface/{ptTriDist2,nearestFaces}.jsonl (direct) and *.captured.jsonl from
// web/prop/nearface.js.
// ptTriDist2    args: [p, a, b, c] (each [x, y, z])                  out: number
// nearestFaces  args: [$topo, rot, offset, px, py, pz]              out: [face index...]
import { writeCases } from "./lib/encode.js";
import { extentXY, FRACS, modelList, posed, POSES, topoRef } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const { ptTriDist2, nearestFaces } = await import(`${WEB}prop/nearface.js`);

const cases = { ptTriDist2: [], nearestFaces: [] };

// ptTriDist2: every Voronoi region of the triangle (vertex a/b/c, edge ab/ac/bc, face) from a fixed
// triangle, plus a seeded scatter and degenerate triangles.
{
  const a = [0, 0, 0], b = [4, 0, 0], c = [0, 3, 0];
  const probes = {
    "vertex-a": [-1, -1, 0.5], "vertex-b": [6, -1, 0], "vertex-c": [-1, 5, 1],
    "edge-ab": [2, -2, 1], "edge-ac": [-2, 1.5, -1], "edge-bc": [4, 3, 0.5],
    "face": [1, 1, 2], "face-on": [1, 1, 0], "on-vertex": [0, 0, 0], "on-edge": [2, 0, 0],
  };
  for (const [tag, p] of Object.entries(probes)) cases.ptTriDist2.push({ id: tag, args: [p, a, b, c], out: ptTriDist2(p, a, b, c) });
  let seed = 12345;
  const rnd = () => ((seed = (Math.imul(seed, 1103515245) + 12345) >>> 0) / 4294967296) * 20 - 10;
  for (let i = 0; i < 400; i++) {
    const tri = [0, 1, 2].map(() => [rnd(), rnd(), rnd()]);
    const p = [rnd(), rnd(), rnd()];
    cases.ptTriDist2.push({ id: `rand${i}`, args: [p, ...tri], out: ptTriDist2(p, ...tri) });
  }
  const z = [0, 0, 0];
  cases.ptTriDist2.push({ id: "degenerate-point", args: [[1, 2, 3], z, z, z], out: ptTriDist2([1, 2, 3], z, z, z) });
  cases.ptTriDist2.push({ id: "degenerate-line", args: [[1, 2, 3], z, [2, 0, 0], [4, 0, 0]], out: ptTriDist2([1, 2, 3], z, [2, 0, 0], [4, 0, 0]) });
  cases.ptTriDist2.push({ id: "nan", args: [[NaN, 0, 0], a, b, c], out: ptTriDist2([NaN, 0, 0], a, b, c) });
  cases.ptTriDist2.push({ id: "inf", args: [[Infinity, 0, 0], a, b, c], out: ptTriDist2([Infinity, 0, 0], a, b, c) });
}

// nearestFaces: a lattice of points around each posed model (inside, on, outside), plus non-finite points.
for (const [name] of modelList()) {
  for (const deg of POSES.slice(0, 3)) {
    const { topo, rot, offset, soup } = posed(name, deg);
    const e = extentXY(soup);
    let zmax = 0;
    for (let i = 2; i < soup.length; i += 3) if (soup[i] > zmax) zmax = soup[i];
    const tag = `${name}|r${deg.join(",")}`;
    for (const fx of [-0.05, 0.3, 0.5, 0.9]) {
      for (const fy of [0.1, 0.5, 1.05]) {
        for (const fz of [-0.05, 0.25, 0.75, 1.1]) {
          const px = e.x0 + fx * (e.x1 - e.x0), py = e.y0 + fy * (e.y1 - e.y0), pz = fz * zmax;
          cases.nearestFaces.push({ id: `${tag}|${px},${py},${pz}`, args: [topoRef(name), rot, offset, px, py, pz], out: nearestFaces(topo, rot, offset, px, py, pz) });
        }
      }
    }
  }
}
{
  const { topo, rot, offset } = posed("cube", [0, 0, 0]);
  for (const [tag, p] of Object.entries({ nan: [NaN, 0, 0], inf: [Infinity, 0, 0], far: [1e9, 1e9, 1e9] })) {
    cases.nearestFaces.push({ id: `cube-${tag}`, args: [topoRef("cube"), rot, offset, ...p], out: nearestFaces(topo, rot, offset, ...p) });
  }
}
for (const [fn, list] of Object.entries(cases)) writeCases("nearface", fn, list);

await recordGolden("nearface", [{ rel: "prop/nearface.js", fns: ["ptTriDist2", "nearestFaces"] }]);
console.log("nearface:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])));
