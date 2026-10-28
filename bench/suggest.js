// M5 suggest timing, the JS side of `just bench-suggest` (the Rust side is
// crates/sf-testkit/examples/bench_suggest.rs, which also writes the report).
//
//   deno run -A bench/suggest.js     time web suggestOrientations ({top: 3, threshold: 45}, default config) on every
//                                    stress model, write target/bench-suggest/js.json
//
// Per model: one discarded warm-up, then the median of RUNS. `candidates` is the size of the web's
// candidateDowns list (that function is module-private, so its logic is repeated here verbatim).
import { UPSTREAM_ROOT, assertPinned } from "../parity/oracle/lib/pin.js";

assertPinned();
const RUNS = 5;
const OUT = new URL("../target/bench-suggest/", import.meta.url).pathname;
const MODELS = `${UPSTREAM_ROOT}/prototype/stress/models/`;
const median = (a) => { const s = [...a].sort((x, y) => x - y); return s[s.length >> 1]; };

const { buildTopology } = await import(`${UPSTREAM_ROOT}/web/overhangs.js`);
const { suggestOrientations } = await import(`${UPSTREAM_ROOT}/web/orient.js`);
const { readSTL } = await import(`${UPSTREAM_ROOT}/web/stl.js`);
const topoOf = (pos) => buildTopology({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });

function candidateCount(topo, maxClusters = 12) { // web/orient.js candidateDowns
  const { nrm, area, nFaces } = topo;
  const bins = new Map();
  const q = (v) => Math.round(v * 12) / 12;
  for (let f = 0; f < nFaces; f++) {
    const x = nrm[f * 3], y = nrm[f * 3 + 1], z = nrm[f * 3 + 2];
    if (x === 0 && y === 0 && z === 0) continue;
    const key = `${q(x)},${q(y)},${q(z)}`;
    const b = bins.get(key);
    if (b) { b.a += area[f]; b.x += x * area[f]; b.y += y * area[f]; b.z += z * area[f]; }
    else bins.set(key, { a: area[f], x: x * area[f], y: y * area[f], z: z * area[f] });
  }
  const clusters = [...bins.values()].sort((p, r) => r.a - p.a).slice(0, maxClusters)
    .map((b) => { const n = Math.hypot(b.x, b.y, b.z); return [b.x / n, b.y / n, b.z / n]; });
  const downs = [...clusters, [1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
  const out = [];
  for (const d of downs) if (!out.some((e) => e[0] * d[0] + e[1] * d[1] + e[2] * d[2] > 0.996)) out.push(d);
  return out.length;
}

// the stress set, then larger parts: the calibration coupons (10-18k triangles) and tests/fixtures/torus_flat
const files = [...Deno.readDirSync(MODELS)].filter((e) => e.name.endsWith(".stl")).sort((a, b) => a.name.localeCompare(b.name))
  .map((e) => [e.name.slice(0, -4), MODELS + e.name]);
for (const c of ["cutout", "honeycomb", "sampler", "tall", "orient", "bore"]) {
  const dir = `${UPSTREAM_ROOT}/prototype/calibration/${c === "honeycomb" ? "cutout" : c}/print/`;
  files.push([`${c}-coupon`, `${dir}${c}-coupon.stl`]);
}
files.push(["torus_flat", `${UPSTREAM_ROOT}/tests/fixtures/torus_flat.stl`]);
const rows = [];
for (const [model, path] of files) {
  const pos = readSTL(Deno.readFileSync(path));
  const run = () => { const topo = topoOf(pos); const t = performance.now(); const r = suggestOrientations(topo, { top: 3, threshold: 45 }); return [performance.now() - t, r, topo]; };
  run();
  const rs = [];
  let last, topo;
  for (let i = 0; i < RUNS; i++) { const [ms, r, t] = run(); rs.push(ms); last = r; topo = t; }
  rows.push({ model, triangles: pos.length / 9, candidates: candidateCount(topo), ms: median(rs), best: last.candidates[0]?.score ?? null, confidence: last.confidence });
  console.log(`${model}: ${rows.at(-1).triangles} tris, ${rows.at(-1).candidates} candidates, ${rows.at(-1).ms.toFixed(1)} ms`);
}
Deno.mkdirSync(OUT, { recursive: true });
Deno.writeTextFileSync(`${OUT}js.json`, JSON.stringify({ deno: Deno.version.deno, runs: RUNS, rows }));
