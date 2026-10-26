// Records parity/corpus/gridsize/gridDims.jsonl (direct calls) and gridDims.captured.jsonl (the calls
// the 54 golden-scene builds make) from web/gridsize.js.
// gridDims  args: [ea, eb, n]  out: [cellsA, cellsB]
import { assertPinned, UPSTREAM_ROOT, WEB } from "./lib/pin.js";
import { writeCases } from "./lib/encode.js";
import { captureModule, flush, setScene, upstreamStatus } from "./lib/capture.js";

assertPinned();
const statusBefore = upstreamStatus();
const { gridDims } = await import(`${WEB}gridsize.js`);

// --- direct: a deterministic grid of extents and face counts, plus the edge values (no NaN or infinite extent: the JS returns NaN cells there and the Rust signature is usize) ---
const extents = [0, -1, 1e-9, 1e-6, 0.1, 0.5, 1, 2.5, 10, 33.3, 64, 100, 250.75, 768, 5000, 1e6];
const counts = [0, 1, 2, 12, 100, 4096, 4097, 250000, 1.5e6, 1e9, 0.5, -3, Infinity];
const cases = [];
const seen = new Set();
for (const ea of extents) {
  for (const eb of extents) {
    for (const n of counts) {
      const id = `${ea}|${eb}|${n}`;
      if (seen.has(id)) continue;
      seen.add(id);
      cases.push({ id, args: [ea, eb, n], out: gridDims(ea, eb, n) });
    }
  }
}
writeCases("gridsize", "gridDims", cases);

// --- captured ---
const goldenDir = `${UPSTREAM_ROOT}/tests/golden`;
const scenes = [...Deno.readDirSync(goldenDir)].filter((e) => e.name.endsWith(".json")).map((e) => e.name).sort()
  .map((n) => JSON.parse(Deno.readTextFileSync(`${goldenDir}/${n}`)).scene);
const cap = await captureModule("gridsize.js", ["gridDims"], {
  copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"],
});
try {
  const { run } = await import(new URL("tests/_scene.js", cap.rootUrl).href);
  for (const scene of scenes) {
    setScene(cap.sink, scene.name);
    run(scene);
  }
  flush("gridsize", cap.sink);
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
console.log(`gridsize: ${cases.length} direct`);
