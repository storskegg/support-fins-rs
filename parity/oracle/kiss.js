// Records parity/corpus/kiss/kissEnds.jsonl (direct) and kissEnds.captured.jsonl (golden scenes)
// from web/kiss.js. Topology is the standard reference {"$topo": id}.
//   kissEnds  args: [topo, rot, offset, x, y, dx, dy, zb, zt, opts]   out: {bot: [l, r], top: [l, r]}
// opts is {half, back, reach} plus, in some cases, contactOffset (a number, NaN or +-Infinity; absent
// means the JS default -0.01).
import { assertPinned, UPSTREAM_ROOT, WEB } from "./lib/pin.js";
import { Ref, writeCases } from "./lib/encode.js";
import { captureModules, flush, setScene, upstreamStatus } from "./lib/capture.js";

assertPinned();
const statusBefore = upstreamStatus();

const SKIP_OVER = 20 * 1024 * 1024;
const POSES = [[0, 0, 0], [90, 0, 0], [-45, 30, 15]];
const OPTS = [
  { half: 0.25, back: 0.3, reach: 1.5 },
  { half: 0.4, back: 0.5, reach: 3 },
  { half: 0.25, back: 0.3, reach: 1.5, contactOffset: 0.1 },
  { half: 0.25, back: 0.3, reach: 1.5, contactOffset: -0.5 },
  { half: 0.25, back: 0.3, reach: 1.5, contactOffset: NaN },
];

const { buildTopology, analyze } = await import(`${WEB}overhangs.js`);
const { kissEnds } = await import(`${WEB}kiss.js`);
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

const HEADINGS = [[0, 1], [1, 0], [-0.6, 0.8], [0.7071067811865476, -0.7071067811865476]];
const cases = [];
for (const [name, path] of models) {
  const pos = readSTL(Deno.readFileSync(path));
  const topo = buildTopology(geometry(pos));
  const ref = new Ref({ $topo: name });
  for (const deg of POSES) {
    const rot = sitePose(deg);
    const off = structuredClone(analyze(topo, 45, rot).offset);
    const nv = pos.length / 3;
    for (let n = 0; n < 5; n++) {
      const v = Math.floor((n * nv) / 5 + 1) % nv;
      const x = pos[v * 3], y = pos[v * 3 + 1], z = pos[v * 3 + 2];
      const px = rot[0] * x + rot[3] * y + rot[6] * z + off.x;
      const py = rot[1] * x + rot[4] * y + rot[7] * z + off.y;
      const pz = rot[2] * x + rot[5] * y + rot[8] * z + off.z;
      HEADINGS.forEach(([dx, dy], h) => {
        const opts = OPTS[(n + h) % OPTS.length];
        // a tine seeded 0.4 mm off the vertex against the heading, 0.6 mm tall around its z
        const args = [ref, rot, off, px - dx * 0.4, py - dy * 0.4, dx, dy, pz - 0.3, pz + 0.3, opts];
        cases.push({ id: `${name}|r${deg.join(",")}|v${n}|h${h}`, args, out: kissEnds(topo, rot, off, ...args.slice(3)) });
      });
    }
  }
}
writeCases("kiss", "kissEnds", cases);

const goldenDir = `${UPSTREAM_ROOT}/tests/golden`;
const scenes = [...Deno.readDirSync(goldenDir)].filter((e) => e.name.endsWith(".json")).map((e) => e.name).sort()
  .map((n) => JSON.parse(Deno.readTextFileSync(`${goldenDir}/${n}`)).scene);
const cap = await captureModules(
  [{ rel: "overhangs.js", fns: ["buildTopology", "analyze"] }, { rel: "kiss.js", fns: ["kissEnds"] }],
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
  flush("kiss", cap.sink);
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
console.log(`kiss: ${cases.length} kissEnds, ${scenes.length} scenes captured`);
