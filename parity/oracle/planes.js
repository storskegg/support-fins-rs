// Records parity/corpus/planes/*.jsonl (direct) and *.captured.jsonl (the 54 golden scenes) from
// web/planes.js. Topology arguments are the standard reference {"$topo": id}; rot is
// sitePose(deg) and offset is analyze(topo, 45, rot).offset.
//   findWallPatches          args: [topo, rot, offset]                 out: the patches array
//   findWallPatches.stats    args: [topo, rot, offset]                 out: the stats object a call fills
//   findWallPatches.excluded args: [topo, rot, offset, null, Uint8Array] out: the patches array
//   faceAdjacency            args: [topo]                              out: {start, nbr}
//   patchProbe.grid          args: [patch, [[u, t], ...]]              out: [probe | null]  (direct only)
//   patchCovers.grid         same, out: [boolean]
//   patchPoint.grid          args: [{d, n, u, t}, [[w, u, t], ...]]    out: [[x, y, z]]
//   tAtZ.grid, zAt.grid      args: [{d, n, u, t}, [[w, z|t], ...]]     out: [number]
// The captured calls (findWallPatches, faceAdjacency, patchProbe, patchPoint; patchCovers is only called inside planes.js, and tAtZ/zAt are `export const` arrows the wrapper cannot rename)
// carry the patch objects in full.
import { assertPinned, UPSTREAM_ROOT, WEB } from "./lib/pin.js";
import { Ref, writeCases } from "./lib/encode.js";
import { captureModules, flush, setScene, upstreamStatus } from "./lib/capture.js";

assertPinned();
const statusBefore = upstreamStatus();

const SKIP_OVER = 2 * 1024 * 1024;
const POSES = [[0, 0, 0], [90, 0, 0], [45, 0, 0], [-45, 30, 15]];

const { buildTopology, analyze } = await import(`${WEB}overhangs.js`);
const P = await import(`${WEB}planes.js`);
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

const plain = (x) => structuredClone(x);
const cases = { find: [], stats: [], excluded: [], adj: [], probe: [], covers: [], point: [], tAtZ: [], zAt: [] };
for (const [name, path] of models) {
  const pos = readSTL(Deno.readFileSync(path));
  const topo = buildTopology(geometry(pos));
  const ref = new Ref({ $topo: name });
  cases.adj.push({ id: name, args: [ref], out: P.faceAdjacency(topo) });
  for (const deg of POSES) {
    const rot = sitePose(deg);
    const off = plain(analyze(topo, 45, rot).offset);
    const id = `${name}|r${deg.join(",")}`;
    const patches = P.findWallPatches(topo, rot, off);
    cases.find.push({ id, args: [ref, rot, off], out: patches });
    const stats = {};
    P.findWallPatches(topo, rot, off, stats);
    cases.stats.push({ id, args: [ref, rot, off], out: stats });
    const ex = new Uint8Array(topo.nFaces);
    for (let f = 0; f < ex.length; f += 7) ex[f] = 1;
    cases.excluded.push({ id, args: [ref, rot, off, null, ex], out: P.findWallPatches(topo, rot, off, null, ex) });
    // the smallest and the largest patch of the pose
    for (const [k, patch] of [[0, patches[0]], [1, patches[patches.length - 1]]]) {
      if (!patch) continue;
      const pid = `${id}|p${k}`;
      const uv = [];
      for (let i = 0; i <= 5; i++) for (let j = 0; j <= 5; j++) {
        uv.push([patch.u0 - 0.5 + ((patch.u1 - patch.u0 + 1) * i) / 5, patch.t0 - 0.5 + ((patch.t1 - patch.t0 + 1) * j) / 5]);
      }
      cases.probe.push({ id: pid, args: [patch, uv], out: uv.map(([u, t]) => P.patchProbe(patch, u, t)) });
      cases.covers.push({ id: pid, args: [patch, uv], out: uv.map(([u, t]) => P.patchCovers(patch, u, t)) });
      const frame = { d: patch.d, n: patch.n, u: patch.u, t: patch.t };
      const wut = [[0, 0, 0], [0.5, 1, 2], [-0.2, -3, 4.5], [1.25, 0.1, -7]];
      cases.point.push({ id: pid, args: [frame, wut], out: wut.map(([w, u, t]) => P.patchPoint(frame, w, u, t)) });
      const wz = [[0, 0], [0.5, 10], [-0.2, -3.5], [1.25, 40]];
      cases.tAtZ.push({ id: pid, args: [frame, wz], out: wz.map(([w, z]) => P.tAtZ(frame, w, z)) });
      cases.zAt.push({ id: pid, args: [frame, wz], out: wz.map(([w, t]) => P.zAt(frame, w, t)) });
    }
  }
}
writeCases("planes", "findWallPatches", cases.find);
writeCases("planes", "findWallPatches.stats", cases.stats);
writeCases("planes", "findWallPatches.excluded", cases.excluded);
writeCases("planes", "faceAdjacency", cases.adj);
writeCases("planes", "patchProbe.grid", cases.probe);
writeCases("planes", "patchCovers.grid", cases.covers);
writeCases("planes", "patchPoint.grid", cases.point);
writeCases("planes", "tAtZ.grid", cases.tAtZ);
writeCases("planes", "zAt.grid", cases.zAt);

// --- captured ---
const goldenDir = `${UPSTREAM_ROOT}/tests/golden`;
const scenes = [...Deno.readDirSync(goldenDir)].filter((e) => e.name.endsWith(".json")).map((e) => e.name).sort()
  .map((n) => JSON.parse(Deno.readTextFileSync(`${goldenDir}/${n}`)).scene);
const cap = await captureModules(
  [{ rel: "overhangs.js", fns: ["buildTopology", "analyze"] },
   { rel: "planes.js", fns: ["findWallPatches", "faceAdjacency", "patchProbe", "patchPoint"] }],
  { refs: true, cap: 60, args: { buildTopology: ([g]) => [g.getAttribute("position").array] },
    copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"] },
);
try {
  const { run } = await import(new URL("tests/_scene.js", cap.rootUrl).href);
  for (const scene of scenes) {
    setScene(cap.sink, scene.name);
    cap.sink.model = scene.model;
    run(scene);
  }
  delete cap.sink.calls.buildTopology;
  delete cap.sink.calls.analyze;
  flush("planes", cap.sink);
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
console.log(`planes: ${cases.find.length} findWallPatches poses, ${scenes.length} scenes captured`);
