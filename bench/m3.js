// M3 timing: the JS engine (buildFins) against the single-threaded Rust engine on the same builds.
//
//   deno run -A bench/m3.js run      time the JS side, write target/bench-m3/js.json (the case list too)
//   (cargo run --release -p sf-testkit --example bench_m3)   time the Rust side on that case list
//   deno run -A bench/m3.js report   merge both into docs/PERF-M3.md
//
// `just bench-m3` runs the three in order. Serial baseline: no parallelism, no tuning.
//
// A case is a model at a pose, built exactly as the site does (tests/_scene.js `siteOpts`: Auto, PLA, the
// form untouched). Per case the phases are timed separately, median of RUNS after one discarded warm-up:
//   load    read the file and parse it (STL / 3MF object) into a position soup
//   weld    buildTopology (the welded topology) + analyze (overhang flags, regions)
//   build   buildFins
// The private coin file contributes only its name, triangle counts and timings: its geometry is read
// from private-fixtures/ at run time and never written anywhere (js.json holds object indices, not geometry).
import { UPSTREAM_ROOT, assertPinned } from "../parity/oracle/lib/pin.js";

assertPinned();
const RUNS = 5;
const OUT = new URL("../target/bench-m3/", import.meta.url).pathname;
const REPO = new URL("../", import.meta.url).pathname;
const PRIVATE = Deno.env.get("BENCH_PRIVATE_3MF") ?? `${REPO}private-fixtures/Earth Focus Many Vertical.3mf`;
const MODELS = `${UPSTREAM_ROOT}/prototype/stress/models/`;

const median = (a) => { const s = [...a].sort((x, y) => x - y); return s[s.length >> 1]; };

async function run() {
  const { buildTopology, analyze } = await import(`${UPSTREAM_ROOT}/web/overhangs.js`);
  const { buildFins } = await import(`${UPSTREAM_ROOT}/web/fins.js`);
  const { readSTL } = await import(`${UPSTREAM_ROOT}/web/stl.js`);
  const { siteOpts, sitePose } = await import(`${UPSTREAM_ROOT}/tests/_scene.js`);
  const { readThreeMF } = await import(`${UPSTREAM_ROOT}/web/threemf.js`);
  const topoOf = (pos) => buildTopology({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });

  // ---- the case list ----------------------------------------------------------------------------
  const d2r = (d) => (d * Math.PI) / 180;
  const mul = (a, b) => { const m = new Array(9).fill(0);
    for (let c = 0; c < 3; c++) for (let r = 0; r < 3; r++) for (let k = 0; k < 3; k++) m[c * 3 + r] += a[k * 3 + r] * b[c * 3 + k]; return m; };
  const rotX = (d) => { const c = Math.cos(d2r(d)), s = Math.sin(d2r(d)); return [1, 0, 0, 0, c, s, 0, -s, c]; };
  const rotY = (d) => { const c = Math.cos(d2r(d)), s = Math.sin(d2r(d)); return [c, 0, -s, 0, 1, 0, s, 0, c]; };
  const rotZ = (d) => { const c = Math.cos(d2r(d)), s = Math.sin(d2r(d)); return [c, s, 0, -s, c, 0, 0, 0, 1]; };
  // prototype/stress/run.js ORIENTS
  const ORIENTS = [["flat", [1, 0, 0, 0, 1, 0, 0, 0, 1]], ["X30", rotX(30)], ["X45", rotX(45)], ["X60", rotX(60)],
    ["Y45", rotY(45)], ["X45Y30", mul(rotY(30), rotX(45))], ["X60Z30", mul(rotZ(30), rotX(60))]];

  const triCount = {};
  for (const e of Deno.readDirSync(MODELS)) if (e.name.endsWith(".stl")) {
    triCount[e.name.slice(0, -4)] = readSTL(Deno.readFileSync(MODELS + e.name)).length / 9;
  }
  const cases = [], seen = new Set();
  const add = (c) => { const k = `${c.model}|${c.rot.join(",")}`; if (!seen.has(k)) { seen.add(k); cases.push(c); } };

  // 1. every stress model at its golden pose(s): the non-draw, non-sway scenes of tests/golden at default settings
  const dir = `${UPSTREAM_ROOT}/tests/golden`;
  const scenes = [...Deno.readDirSync(dir)].filter((e) => e.name.endsWith(".json")).map((e) => e.name).sort()
    .map((n) => JSON.parse(Deno.readTextFileSync(`${dir}/${n}`)).scene)
    .filter((s) => !s.draw && !s.set && triCount[s.model] !== undefined);
  for (const s of scenes) add({ group: "golden", model: s.model, pose: s.rot.every((d) => !d) ? "up" : `X${s.rot[0]}${s.rot[1] ? `Y${s.rot[1]}` : ""}${s.rot[2] ? `Z${s.rot[2]}` : ""}`, rot: sitePose(s.rot) });

  // 2. worst orientations: every stress pose of the five largest stress models
  const large = Object.entries(triCount).sort((a, b) => b[1] - a[1]).slice(0, 5).map(([n]) => n);
  for (const m of large) for (const [name, rot] of ORIENTS) add({ group: "worst", model: m, pose: name, rot });

  // 3. the private coin file, each object at the pose it has in the file (read now, kept only in memory)
  let privateObjects = 0;
  try {
    const bytes = Deno.readFileSync(PRIVATE);
    privateObjects = (await readThreeMF(bytes)).objects.length;
    for (let i = 0; i < privateObjects; i++) {
      cases.push({ group: "private", model: "Earth Focus Many Vertical", pose: `object ${i + 1}`, rot: [1, 0, 0, 0, 1, 0, 0, 0, 1], object: i });
    }
  } catch (e) {
    if (!(e instanceof Deno.errors.NotFound)) throw e;
    console.log(`bench-m3: ${PRIVATE} is absent; the private coin cases are skipped`);
  }

  // ---- timing -----------------------------------------------------------------------------------
  const opts = siteOpts();
  const privateBytes = privateObjects ? Deno.readFileSync(PRIVATE) : null;
  const load = async (c) => c.group === "private" ? (await readThreeMF(privateBytes)).objects[c.object].positions
    : readSTL(Deno.readFileSync(`${MODELS}${c.model}.stl`));
  const once = async (c) => {
    let t = performance.now();
    const pos = await load(c);
    const tLoad = performance.now() - t; t = performance.now();
    const topo = topoOf(pos);
    const res = analyze(topo, 45, c.rot);
    const tWeld = performance.now() - t; t = performance.now();
    const built = buildFins(topo, res, c.rot, opts);
    const tBuild = performance.now() - t;
    return { load: tLoad, weld: tWeld, build: tBuild, triangles: pos.length / 9, out: built.triangles.length };
  };
  const rows = [];
  for (const c of cases) {
    await once(c); // warm-up, discarded
    const r = [];
    for (let i = 0; i < RUNS; i++) r.push(await once(c));
    rows.push({ ...c, triangles: r[0].triangles, outVerts: r[0].out,
      load: median(r.map((x) => x.load)), weld: median(r.map((x) => x.weld)), build: median(r.map((x) => x.build)) });
    console.log(`${c.group} ${c.model} ${c.pose}: ${r[0].triangles} tris, build ${rows.at(-1).build.toFixed(1)} ms`);
  }
  Deno.mkdirSync(OUT, { recursive: true });
  const dec = new TextDecoder();
  const sh = (cmd, ...args) => { try { return dec.decode(new Deno.Command(cmd, { args, stdout: "piped" }).outputSync().stdout).trim(); } catch { return "?"; } };
  const meta = { runs: RUNS, deno: Deno.version.deno, chip: sh("sysctl", "-n", "machdep.cpu.brand_string"),
    os: `macOS ${sh("sw_vers", "-productVersion")}`, large, upstream: Deno.readTextFileSync(`${REPO}UPSTREAM`).match(/commit=(\w+)/)[1],
    privateFile: privateObjects ? PRIVATE.split("/").pop() : null };
  Deno.writeTextFileSync(`${OUT}js.json`, JSON.stringify({ meta, cases: rows }));
}

function report() {
  const js = JSON.parse(Deno.readTextFileSync(`${OUT}js.json`));
  const rs = JSON.parse(Deno.readTextFileSync(`${OUT}rust.json`));
  const f = (ms) => (ms < 10 ? ms.toFixed(2) : ms < 100 ? ms.toFixed(1) : ms.toFixed(0));
  const ratio = (a, b) => (b > 0 ? `${(a / b).toFixed(1)}x` : "-");
  const table = (group) => {
    const out = ["| model | pose | triangles | JS ms | Rust ms | ratio | JS load / weld ms | Rust load / weld ms |", "|---|---|---:|---:|---:|---:|---:|---:|"];
    const sel = js.cases.map((c, i) => [c, rs.cases[i]]).filter(([c]) => c.group === group);
    for (const [c, r] of sel) {
      out.push(`| ${c.model} | ${c.pose} | ${c.triangles} | ${f(c.build)} | ${f(r.build)} | ${ratio(c.build, r.build)} | ${f(c.load)} / ${f(c.weld)} | ${f(r.load)} / ${f(r.weld)} |`);
    }
    const jb = sel.reduce((s, [c]) => s + c.build, 0), rb = sel.reduce((s, [, r]) => s + r.build, 0);
    return { text: out.join("\n"), n: sel.length, jb, rb, med: median(sel.map(([c, r]) => c.build / r.build)) };
  };
  const g = table("golden"), w = table("worst"), p = table("private");
  const same = js.cases.every((c, i) => rs.cases[i].outVerts === c.outVerts);
  const mism = js.cases.filter((c, i) => rs.cases[i].outVerts !== c.outVerts).length;
  const sum = (t, label) => `${label}: ${t.n} builds, total JS ${f(t.jb)} ms vs Rust ${f(t.rb)} ms (${ratio(t.jb, t.rb)} overall, median per-build ratio ${t.med.toFixed(1)}x)`;
  const md = `# M3 timing: JS engine vs single-threaded Rust engine

The serial baseline M5 improves on. Regenerate with \`just bench-m3\` (writes this file).

- **Machine:** ${js.meta.chip}, ${js.meta.os}.
- **JS:** Deno ${js.meta.deno}, \`buildFins\` from the pinned upstream (${js.meta.upstream.slice(0, 7)}). **Rust:** ${rs.meta.rustc}, \`cargo build --release\`, \`build_fins\`, one thread.
- **Builds:** the site's own request (\`tests/_scene.js\` \`siteOpts()\`: Auto, PLA, tines, bed pad, the form untouched), threshold 45.
- **Method:** each case is timed ${js.meta.runs} times after one discarded warm-up; the table gives the median. The phases are separate:
  **load** = read the file and parse it into a position soup, **weld** = \`buildTopology\` + \`analyze\`, and the **JS ms / Rust ms** columns are \`buildFins\` alone (the ratio is JS over Rust, so above 1 means Rust is faster).
  Every run starts from a fresh topology, so lazily built grids are charged to the build.
- **Same work:** the Rust build produced the same support vertex count as JS in ${js.cases.length - mism} of ${js.cases.length} cases${same ? "" : ` (${mism} differ; see below)`}. Both engines are tested for exact equality in the parity corpus, so these timings compare equal work.

## Summary

- ${sum(g, "Stress models at their golden poses")}
- ${sum(w, "Worst orientations of the " + js.meta.large.join(", ") + " models")}
${p.n ? `- ${sum(p, "Private coin file")}` : "- Private coin file: not present on this machine (skipped)."}

## Stress models at their golden poses

Every stress model at the poses of its non-draw, non-sway \`tests/golden\` scenes ("up" = as modelled, \`X40\` = tilted 40 degrees), at default settings.

${g.text}

## Worst orientations

The five stress models with the most triangles (${js.meta.large.join(", ")}) in each pose of \`prototype/stress/run.js\` (flat, X30, X45, X60, Y45, X45Y30, X60Z30), other than the ones above.

${w.text}
${p.n ? `
## Private coin file (${js.meta.privateFile})

Each object at the pose it has in the file. Only counts and timings are recorded here; none of its geometry, outputs or corpus is stored.

${p.text}
` : ""}`;
  Deno.writeTextFileSync(`${REPO}docs/PERF-M3.md`, md);
  console.log(`wrote docs/PERF-M3.md\n${sum(g, "golden")}\n${sum(w, "worst")}${p.n ? "\n" + sum(p, "private") : ""}`);
}

const cmd = Deno.args[0];
if (cmd === "run") await run();
else if (cmd === "report") report();
else { console.error("usage: m3.js run|report"); Deno.exit(2); }
