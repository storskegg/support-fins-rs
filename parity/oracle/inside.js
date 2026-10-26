// Records parity/corpus/inside/{insidePart,nearestPart,solidClearance}.jsonl (direct calls) and
// *.captured.jsonl (calls the 54 golden-scene builds make) from web/inside.js.
//
// The topology argument is the standard reference {"$topo": "<model id>"} (docs/PARITY.md); the
// Rust replay rebuilds it with sf_testkit::parity::topo. rot is sitePose([x,y,z]) (9 numbers),
// offset is analyze(topo, 45, rot).offset ({x, y, z}).
//   insidePart      args: [topo, rot, offset, qx, qy, qz]            out: boolean
//   nearestPart     args: [topo, rot, offset, qx, qy, qz, rMax]      out: {d, cosUp} or null
//   solidClearance  args: [topo, rot, offset, verts, rMax]           out: {d, cosUp, x, y, z} or null
//                   verts: an array of [x, y, z] (a small box soup in print space, 12 triangles)
import { assertPinned, UPSTREAM_ROOT, WEB } from "./lib/pin.js";
import { Ref, writeCases } from "./lib/encode.js";
import { captureModules, flush, setScene, upstreamStatus } from "./lib/capture.js";

assertPinned();
const statusBefore = upstreamStatus();

const SKIP_OVER = 20 * 1024 * 1024;
const POSES = [[0, 0, 0], [90, 0, 0], [-45, 30, 15]];

const { buildTopology, analyze } = await import(`${WEB}overhangs.js`);
const { insidePart, nearestPart, solidClearance } = await import(`${WEB}inside.js`);
const { readSTL } = await import(`${WEB}stl.js`);
const { sitePose } = await import(`${WEB}../tests/_scene.js`);
const geometry = (pos) => ({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });

const models = [];
function addDir(dir, prefix) {
  const names = [...Deno.readDirSync(dir)].filter((e) => e.isFile && e.name.endsWith(".stl")).map((e) => e.name).sort();
  for (const n of names) {
    const path = `${dir}/${n}`;
    if (Deno.statSync(path).size <= SKIP_OVER) models.push([prefix + n.replace(/\.stl$/, ""), path]);
  }
}
addDir(`${UPSTREAM_ROOT}/prototype/stress/models`, "");
addDir(`${UPSTREAM_ROOT}/tests/fixtures`, "fixtures/");

/** A box soup [x0,x1]x[y0,y1]x[z0,z1], 12 triangles, as an array of vertex triples. */
function box(x0, x1, y0, y1, z0, z1) {
  const p = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
  const t = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [2, 3, 7], [2, 7, 6], [1, 2, 6], [1, 6, 5], [3, 0, 4], [3, 4, 7]];
  return t.flatMap((tri) => tri.map((i) => p[i]));
}

const inside = [], nearest = [], clearance = [];
for (const [name, path] of models) {
  const pos = readSTL(Deno.readFileSync(path));
  const topo = buildTopology(geometry(pos));
  const ref = new Ref({ $topo: name });
  for (const deg of POSES) {
    const rot = sitePose(deg);
    const res = analyze(topo, 45, rot);
    const off = { x: res.offset.x, y: res.offset.y, z: res.offset.z };
    const { x: sx, y: sy, z: sz } = res.size;
    // 4 x 4 x 4 points over the seated part's box, grown by 10%
    const pts = [];
    for (let i = 0; i < 4; i++) for (let j = 0; j < 4; j++) for (let k = 0; k < 4; k++) {
      pts.push([(-0.6 + (1.2 * i) / 3) * sx, (-0.6 + (1.2 * j) / 3) * sy, (-0.1 + (1.2 * k) / 3) * sz]);
    }
    // near-surface points: mesh vertices in print space, nudged along z
    const nf = pos.length / 9;
    for (let n = 0; n < 6; n++) {
      const v = Math.floor((n * nf * 3) / 6 + 1) % (nf * 3);
      const x = pos[v * 3], y = pos[v * 3 + 1], z = pos[v * 3 + 2];
      const px = rot[0] * x + rot[3] * y + rot[6] * z + off.x;
      const py = rot[1] * x + rot[4] * y + rot[7] * z + off.y;
      const pz = rot[2] * x + rot[5] * y + rot[8] * z + off.z;
      for (const dz of [0, 0.1, -0.1]) pts.push([px, py, pz + dz]);
    }
    const tag = `${name}|r${deg.join(",")}`;
    pts.forEach(([qx, qy, qz], n) => {
      inside.push({ id: `${tag}|${n}`, args: [ref, rot, off, qx, qy, qz], out: insidePart(topo, rot, off, qx, qy, qz) });
      for (const rMax of [0.45, 2]) {
        nearest.push({ id: `${tag}|${n}|r${rMax}`, args: [ref, rot, off, qx, qy, qz, rMax], out: nearestPart(topo, rot, off, qx, qy, qz, rMax) });
      }
    });
    // boxes: small cubes at and just off the near-surface points (so most miss, some graze, a few
    // pierce), cubes at a few grid points, plus one slab through the part
    const near = pts.slice(64);
    const boxes = [];
    near.forEach(([cx, cy, cz], n) => {
      if (n % 3 !== 0) return;
      for (const [h, dz] of [[0.3, 0.55], [0.3, -0.7], [0.3, 1.1], [1, 0]]) boxes.push(box(cx - h, cx + h, cy - h, cy + h, cz + dz - h, cz + dz + h));
    });
    pts.slice(0, 64).filter((_, n) => n % 9 === 0).forEach(([cx, cy, cz]) => boxes.push(box(cx - 1, cx + 1, cy - 1, cy + 1, cz - 1, cz + 1)));
    boxes.push(box(-sx, 2 * sx, sy * 0.4, sy * 0.4 + 0.5, -1, sz + 1));
    boxes.forEach((verts, n) => {
      for (const rMax of [0.45, 3]) {
        clearance.push({ id: `${tag}|b${n}|r${rMax}`, args: [ref, rot, off, verts, rMax], out: solidClearance(topo, rot, off, verts, rMax) });
      }
    });
  }
}
writeCases("inside", "insidePart", inside);
writeCases("inside", "nearestPart", nearest);
writeCases("inside", "solidClearance", clearance);

// --- captured: the 54 golden scenes ---
const goldenDir = `${UPSTREAM_ROOT}/tests/golden`;
const scenes = [...Deno.readDirSync(goldenDir)].filter((e) => e.name.endsWith(".json")).map((e) => e.name).sort()
  .map((n) => JSON.parse(Deno.readTextFileSync(`${goldenDir}/${n}`)).scene);
const cap = await captureModules(
  [{ rel: "overhangs.js", fns: ["buildTopology", "analyze"] }, { rel: "inside.js", fns: ["insidePart", "nearestPart", "solidClearance"] }],
  { refs: true, cap: 150, args: { buildTopology: ([g]) => [g.getAttribute("position").array] }, copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"] },
);
try {
  const { run } = await import(new URL("tests/_scene.js", cap.rootUrl).href);
  for (const scene of scenes) {
    setScene(cap.sink, scene.name);
    cap.sink.model = scene.model;
    run(scene);
  }
  // only inside.js's calls are this module's corpus
  delete cap.sink.calls.buildTopology;
  delete cap.sink.calls.analyze;
  flush("inside", cap.sink);
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
console.log(`inside: ${inside.length} insidePart, ${nearest.length} nearestPart, ${clearance.length} solidClearance, ${scenes.length} scenes captured`);
