// Records parity/corpus/islands/findIslands*.jsonl from web/islands.js.
//   findIslands           args: [topo, analysis, rot, opts?]   out: [{z, area, points, x, y}]
//                         topo/analysis are the standard references; captured calls also carry a trailing
//                         {"$config": {"ISLAND": {...}}} snapshot. Direct cases use the default ISLAND.
//   findIslands.soup      args: [pos (Float32Array), thresholdDeg, rot, opts?]   out: as above
//                         synthetic parts (the islands.test.js scenarios and a knife edge); the Rust
//                         replay builds the topology and the analysis itself.
import { assertPinned, UPSTREAM_ROOT, WEB } from "./lib/pin.js";
import { Ref, encode, writeCases } from "./lib/encode.js";
import { captureModules, flush, setScene, upstreamStatus } from "./lib/capture.js";

assertPinned();
const statusBefore = upstreamStatus();

const SKIP_OVER = 5 * 1024 * 1024;
const POSES = [[0, 0, 0], [90, 0, 0], [45, 0, 0], [-45, 30, 15], [0, 90, 0]];
const I3 = [1, 0, 0, 0, 1, 0, 0, 0, 1];

const { buildTopology, analyze } = await import(`${WEB}overhangs.js`);
const { findIslands } = await import(`${WEB}islands.js`);
const { readSTL } = await import(`${WEB}stl.js`);
const { sitePose } = await import(`${WEB}../tests/_scene.js`);
const { block } = await import(`file://${UPSTREAM_ROOT}/tests/_util.js`);
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

const direct = [];
let withIslands = 0;
for (const [name, path] of models) {
  const pos = readSTL(Deno.readFileSync(path));
  const topo = buildTopology(geometry(pos));
  for (const deg of POSES) {
    const rot = sitePose(deg);
    const res = analyze(topo, 45, rot);
    const aref = new Ref({ $analysis: { model: name, threshold: 45, rot: encode(rot) } });
    const tref = new Ref({ $topo: name });
    for (const [tag, opts] of [["", undefined], ["|minArea", { minArea: 0.5 }]]) {
      const args = opts ? [tref, aref, rot, opts] : [tref, aref, rot];
      const out = findIslands(topo, res, rot, opts);
      if (out.length) withIslands++;
      direct.push({ id: `${name}|r${deg.join(",")}${tag}`, args, out });
    }
  }
}
writeCases("islands", "findIslands", direct);

// --- synthetic soups ---
function cornerCube(cx, cy, cz, side) {
  const raw = block(-side / 2, side / 2, -side / 2, side / 2, -side / 2, side / 2);
  const d = [1, 1, 1].map((v) => v / Math.sqrt(3));
  const k = [d[1], -d[0], 0], s = Math.hypot(...k), c = d[2];
  const [kx, ky] = [k[0] / s, k[1] / s];
  const out = new Float32Array(raw.length);
  for (let i = 0; i < raw.length; i += 3) {
    const p = [raw[i], raw[i + 1], raw[i + 2]];
    const kp = [ky * p[2], -kx * p[2], kx * p[1] - ky * p[0]];
    const kd = kx * p[0] + ky * p[1];
    for (let j = 0; j < 3; j++) out[i + j] = p[j] * c + kp[j] * s + [kx, ky, 0][j] * kd * (1 - c);
    out[i] += cx; out[i + 1] += cy; out[i + 2] += cz;
  }
  return out;
}
const join = (...parts) => {
  const pos = new Float32Array(parts.reduce((t, p) => t + p.length, 0));
  let o = 0;
  for (const p of parts) { pos.set(p, o); o += p.length; }
  return pos;
};
/** A prism with its apex edge pointing down: 60-degree faces (not red), `n` vertices along the edge. */
function knife(x0, x1, n, flip) {
  const s = 10 * Math.tan(Math.PI / 6), t = [];
  const A = (i) => [x0 + ((x1 - x0) * i) / n, 0, 10], L = (i) => [x0 + ((x1 - x0) * i) / n, -s, 20], R = (i) => [x0 + ((x1 - x0) * i) / n, s, 20];
  const tri = (a, b, c) => t.push(...(flip ? [a, c, b] : [a, b, c]).flat());
  for (let i = 0; i < n; i++) {
    tri(A(i), L(i + 1), A(i + 1)); tri(A(i), L(i), L(i + 1));       // left face
    tri(A(i), A(i + 1), R(i + 1)); tri(A(i), R(i + 1), R(i));       // right face
    tri(L(i), R(i), R(i + 1)); tri(L(i), R(i + 1), L(i + 1));       // top
  }
  tri(A(0), R(0), L(0)); tri(A(n), L(n), R(n));                      // end caps
  return Float32Array.from(t);
}
const LOW = 20 - (8 * Math.sqrt(3)) / 2;
const pillar = block(0, 10, 0, 10, 0, 30);
let knifeFlip = false;
{
  const probe = knife(0, 40, 4, false);
  const topo = buildTopology(geometry(probe));
  knifeFlip = findIslands(topo, analyze(topo, 45, I3), I3).length === 0;
}
const soups = [
  ["cornerCube-on-pillar", join(pillar, cornerCube(15, 5, 20, 8))],
  ["slab-under-corner", join(pillar, cornerCube(15, 5, 20, 8), block(10, 25, 0, 10, 0, LOW - 0.1))],
  ["slab-short-of-corner", join(pillar, cornerCube(15, 5, 20, 8), block(10, 14.5, 0, 10, 0, LOW - 0.1))],
  ["two-corner-cubes", join(pillar, cornerCube(15, 5, 20, 8), cornerCube(15, 5, 26, 6))],
  ["two-far-corner-cubes", join(pillar, cornerCube(15, 5, 20, 8), cornerCube(-15, 5, 24, 8))],
  ["cornerCube-alone", cornerCube(0, 0, 12, 8)],
  ["knife-5", knife(0, 40, 5, knifeFlip)],
  ["knife-100", knife(0, 40, 100, knifeFlip)],
  ["knife-on-pillar", join(knife(20, 60, 50, knifeFlip), block(0, 5, 0, 5, 0, 30))],
  ["blocks-only", join(block(0, 10, 0, 10, 0, 10), block(20, 30, 0, 10, 0, 10))],
];
const OPTS = [undefined, { minArea: 0.5 }, { layer: 0.05, grace: 0 }, { grace: 1.5, merge: 20 }, { maxFaces: 2 }, { plateZ: 20 }];
const soupCases = [];
for (const [name, pos] of soups) {
  const topo = buildTopology(geometry(pos));
  for (const deg of [[0, 0, 0], [30, 0, 0]]) {
    const rot = sitePose(deg);
    const res = analyze(topo, 45, rot);
    OPTS.forEach((opts, k) => {
      const args = opts ? [pos, 45, rot, opts] : [pos, 45, rot];
      soupCases.push({ id: `${name}|r${deg.join(",")}|o${k}`, args, out: findIslands(topo, res, rot, opts) });
    });
  }
}
writeCases("islands", "findIslands.soup", soupCases);
const soupHits = soupCases.filter((c) => c.out.length).length;
if (!soupHits) throw new Error("no synthetic soup produced an island: the scenes are wrong");

// --- captured ---
const goldenDir = `${UPSTREAM_ROOT}/tests/golden`;
const scenes = [...Deno.readDirSync(goldenDir)].filter((e) => e.name.endsWith(".json")).map((e) => e.name).sort()
  .map((n) => JSON.parse(Deno.readTextFileSync(`${goldenDir}/${n}`)).scene);
let islandsMod;
const cap = await captureModules(
  [{ rel: "overhangs.js", fns: ["buildTopology", "analyze"] }, { rel: "islands.js", fns: ["findIslands"] }],
  { refs: true, cap: 150, args: { buildTopology: ([g]) => [g.getAttribute("position").array] },
    config: { findIslands: ["ISLAND"] }, configs: { ISLAND: () => islandsMod.ISLAND },
    copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"] },
);
islandsMod = cap.modules["islands.js"];
try {
  const { run } = await import(new URL("tests/_scene.js", cap.rootUrl).href);
  for (const scene of scenes) {
    setScene(cap.sink, scene.name);
    cap.sink.model = scene.model;
    run(scene);
  }
  delete cap.sink.calls.buildTopology;
  delete cap.sink.calls.analyze;
  // the golden scenes never call findIslands (no scene passes the island-start fill): no file then
  for (const k of Object.keys(cap.sink.calls)) if (!cap.sink.calls[k].length) delete cap.sink.calls[k];
  flush("islands", cap.sink);
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
console.log(`islands: ${direct.length} direct (${withIslands} with islands), ${soupCases.length} soups (${soupHits} with islands), ${scenes.length} scenes`);
