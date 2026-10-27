// Records parity/corpus/pieces/floatingPieces*.jsonl from web/pieces.js.
//   floatingPieces        args: [topo, analysis, rot]   out: [{faces, lowest, drop}]
//                         standard references (docs/PARITY.md), direct on the stress models and fixtures
//                         and captured from the 54 golden scenes.
//   floatingPieces.soup   args: [pos (Float32Array), thresholdDeg, rot]   out: as above
//                         synthetic multi-body parts (the floating.test.js scenarios and more); the Rust
//                         replay builds the topology and the analysis itself.
import { assertPinned, UPSTREAM_ROOT, WEB } from "./lib/pin.js";
import { Ref, encode, writeCases } from "./lib/encode.js";
import { captureModules, flush, setScene, upstreamStatus } from "./lib/capture.js";

assertPinned();
const statusBefore = upstreamStatus();

const SKIP_OVER = 5 * 1024 * 1024;
const POSES = [[0, 0, 0], [90, 0, 0], [45, 0, 0], [-45, 30, 15]];
const I3 = [1, 0, 0, 0, 1, 0, 0, 0, 1];

const { buildTopology, analyze } = await import(`${WEB}overhangs.js`);
const { floatingPieces } = await import(`${WEB}pieces.js`);
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

const direct = [];
for (const [name, path] of models) {
  const pos = readSTL(Deno.readFileSync(path));
  const topo = buildTopology(geometry(pos));
  for (const deg of POSES) {
    const rot = sitePose(deg);
    const res = analyze(topo, 45, rot);
    const args = [new Ref({ $topo: name }), new Ref({ $analysis: { model: name, threshold: 45, rot: encode(rot) } }), rot];
    direct.push({ id: `${name}|r${deg.join(",")}`, args, out: floatingPieces(topo, res, rot) });
  }
}
writeCases("pieces", "floatingPieces", direct);

// --- synthetic soups (the floating.test.js blocks, as triangle soups) ---
const quad = (a, b, c, d) => [a, b, c, a, c, d];
function block(x0, x1, y0, y1, z0, z1) {
  const v = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
  return [...quad(v[0], v[3], v[2], v[1]), ...quad(v[4], v[5], v[6], v[7]), ...quad(v[0], v[1], v[5], v[4]),
    ...quad(v[2], v[3], v[7], v[6]), ...quad(v[1], v[2], v[6], v[5]), ...quad(v[0], v[4], v[7], v[3])];
}
const cavity = (...b) => {
  const t = block(...b);
  for (let i = 0; i < t.length; i += 3) [t[i + 1], t[i + 2]] = [t[i + 2], t[i + 1]];
  return t;
};
const soup = (tris) => Float32Array.from(tris.flat());
const grid = [];
for (let i = 0; i < 12; i++) for (let j = 0; j < 8; j++) grid.push(...block(i * 3, i * 3 + 2, j * 3, j * 3 + 2, 0, 4), ...block(i * 3, i * 3 + 2, j * 3, j * 3 + 2, 4.1, 5));
const soups = [
  ["foot-and-hanging-bar", [...block(-15, 15, -10, 10, 0, 8), ...block(-15, 15, -10, 10, 13, 20)]],
  ["beside-over-plate", [...block(-15, -5, -10, 10, 0, 8), ...block(5, 15, -10, 10, 6, 12)]],
  ["single-block", block(-15, 15, -10, 10, 0, 8)],
  ["stack-0.1-gap", [...block(-15, 15, -10, 10, 0, 8), ...block(-10, 10, -5, 5, 8.1, 14)]],
  ["stack-0.5-gap", [...block(-15, 15, -10, 10, 0, 8), ...block(-10, 10, -5, 5, 8.5, 14)]],
  ["hollow-box", [...block(-15, 15, -15, 15, 0, 30), ...cavity(-10, 10, -10, 10, 5, 25)]],
  ["loose-piece-in-void", [...block(-15, 15, -15, 15, 0, 30), ...cavity(-10, 10, -10, 10, 5, 25), ...block(-4, 4, -4, 4, 8, 14)]],
  ["two-floaters", [...block(-15, 15, -10, 10, 0, 8), ...block(-15, -5, -10, 10, 13, 20), ...block(5, 15, -10, 10, 11, 20)]],
  ["posts-and-caps", grid],
  ["two-on-plate", [...block(0, 10, 0, 10, 0, 10), ...block(20, 30, 0, 10, 0, 10)]],
];
const soupCases = [];
for (const [name, tris] of soups) {
  const pos = soup(tris);
  const topo = buildTopology(geometry(pos));
  for (const deg of [[0, 0, 0], [90, 0, 0], [30, 0, 20]]) {
    const rot = sitePose(deg);
    soupCases.push({ id: `${name}|r${deg.join(",")}`, args: [pos, 45, rot], out: floatingPieces(topo, analyze(topo, 45, rot), rot) });
  }
}
writeCases("pieces", "floatingPieces.soup", soupCases);
const hits = soupCases.filter((c) => c.out.length).length;
if (!hits) throw new Error("no synthetic soup floats a piece: the scenes are wrong");

// --- captured ---
const goldenDir = `${UPSTREAM_ROOT}/tests/golden`;
const scenes = [...Deno.readDirSync(goldenDir)].filter((e) => e.name.endsWith(".json")).map((e) => e.name).sort()
  .map((n) => JSON.parse(Deno.readTextFileSync(`${goldenDir}/${n}`)).scene);
const cap = await captureModules(
  [{ rel: "overhangs.js", fns: ["buildTopology", "analyze"] }, { rel: "pieces.js", fns: ["floatingPieces"] }],
  { refs: true, cap: 150, args: { buildTopology: ([g]) => [g.getAttribute("position").array] },
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
  for (const k of Object.keys(cap.sink.calls)) if (!cap.sink.calls[k].length) delete cap.sink.calls[k];
  flush("pieces", cap.sink);
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
console.log(`pieces: ${direct.length} direct, ${soupCases.length} soups (${hits} with floating pieces), ${scenes.length} scenes`);
