// Records parity/corpus/overhangs/{buildTopology,analyze}.jsonl (direct calls) and
// {buildTopology,analyze}.captured.jsonl (calls a real golden-scene build makes), from web/overhangs.js.
//
// buildTopology  args: [pos]  (the Float32Array a readSTL gave; the JS takes {getAttribute} and reads
//                'position', so the geometry shim is rebuilt here)   out: the whole topo object
// analyze        args: [model, thresholdDeg, rot]  out: the result. `model` names the STL (the replay
//                rebuilds its topo through the Rust buildTopology): a stress model by bare name, a
//                tests/fixtures STL as 'fixtures/<name>'. rot is sitePose([x,y,z]) (a 9-array).
// *.captured: same shapes (analyze's topo is replaced by the model of the scene being run); ids are
//                '<scene name>#<call index in that scene>'.
import { assertPinned, UPSTREAM_ROOT, WEB } from "./lib/pin.js";
import { writeCases } from "./lib/encode.js";
import { captureModule, flush, setScene, upstreamStatus } from "./lib/capture.js";

assertPinned();
const statusBefore = upstreamStatus();

const SKIP_OVER = 20 * 1024 * 1024;
const THRESHOLDS = [30, 45, 60];
const POSES = [[0, 0, 0], [90, 0, 0], [0, 90, 0], [45, 0, 0], [35, 0, 0], [0, 35, 20], [180, 0, 0], [-45, 30, 15]];

const { buildTopology, analyze } = await import(`${WEB}overhangs.js`);
const { readSTL } = await import(`${WEB}stl.js`);
const { sitePose } = await import(`${WEB}../tests/_scene.js`);

const geometry = (pos) => ({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });

// --- the models: [name, path] ---
const models = [];
const skipped = [];
function addDir(dir, prefix) {
  const names = [...Deno.readDirSync(dir)].filter((e) => e.isFile && e.name.endsWith(".stl")).map((e) => e.name).sort();
  for (const n of names) {
    const path = `${dir}/${n}`;
    if (Deno.statSync(path).size > SKIP_OVER) skipped.push(path);
    else models.push([prefix + n.replace(/\.stl$/, ""), path]);
  }
}
addDir(`${UPSTREAM_ROOT}/prototype/stress/models`, "");
addDir(`${UPSTREAM_ROOT}/tests/fixtures`, "fixtures/");

// --- direct cases ---
const topoCases = [];
const analyzeCases = [];
for (const [name, path] of models) {
  const pos = readSTL(Deno.readFileSync(path));
  topoCases.push({ id: name, args: [pos], out: buildTopology(geometry(pos)) });
  const topo = buildTopology(geometry(pos));
  for (const th of THRESHOLDS) {
    for (const deg of POSES) {
      const rot = sitePose(deg);
      // snapshot: analyze() returns the topo's scratch arrays, which the next call overwrites
      analyzeCases.push({ id: `${name}|t${th}|r${deg.join(",")}`, args: [name, th, rot], out: structuredClone(analyze(topo, th, rot)) });
    }
  }
}
writeCases("overhangs", "buildTopology", topoCases);
writeCases("overhangs", "analyze", analyzeCases);

// --- captured cases: the 54 golden scenes through a copy of the tree with overhangs.js wrapped ---
const goldenDir = `${UPSTREAM_ROOT}/tests/golden`;
const scenes = [...Deno.readDirSync(goldenDir)].filter((e) => e.name.endsWith(".json")).map((e) => e.name).sort()
  .map((n) => JSON.parse(Deno.readTextFileSync(`${goldenDir}/${n}`)).scene);

const cap = await captureModule("overhangs.js", ["buildTopology", "analyze"], {
  copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"],
  args: {
    buildTopology: ([g]) => [g.getAttribute("position").array],
    analyze: ([, th, rot], sink) => [sink.model, th, rot],
  },
});
try {
  const { run } = await import(new URL("tests/_scene.js", cap.rootUrl).href);
  for (const scene of scenes) {
    setScene(cap.sink, scene.name);
    cap.sink.model = scene.model;
    run(scene);
  }
  flush("overhangs", cap.sink);
} finally {
  cap.cleanup();
}

if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
if (skipped.length) console.log(`skipped (over 20 MB): ${skipped.join(", ")}`);
console.log(`overhangs: ${topoCases.length} buildTopology, ${analyzeCases.length} analyze, ${scenes.length} scenes captured`);
