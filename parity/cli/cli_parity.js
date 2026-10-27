// CLI parity: run the JS command line (plugins/cli/support-fins.js at UPSTREAM) and the Rust
// `support-fins` on the same arguments and inputs, in separate scratch directories, and require the
// same exit code, the same stdout and stderr (line for line) and the same output files, byte for byte.
//
//   deno run -A parity/cli/cli_parity.js [path/to/support-fins] [case-id-substring]
//
// `just cli-parity` builds the release binary and runs this. Not compared, by design:
//   - the message of a file that cannot be read (the runtime's own: Deno's vs Rust's), only that both
//     fail the same way (exit code, stdout, and the `support-fins: <file>:` prefix);
//
// PROJECT MODE (`--preserve-project`): the output is a ZIP with random identifiers (`crypto.randomUUID()`
// names the support resources). Two runs of the SAME program differ there, so a project output is compared
// member by member: the names and their order, the archive comment, and for each member its raw bytes (a
// member neither program touched must be byte-identical, compressed payload included) or, where they differ,
// its inflated text with every UUID replaced by its order of first appearance in that member
// (`UUID#1`, `UUID#2`, ...). Anything else that differs fails. The run directory and a resume cache's key
// are replaced by placeholders in stdout and stderr.
//
// EXPECTED GAPS (GAPS below): 3MF files with a slicer's negative parts. The JS CLI cuts them with
// Manifold (web/cut.js); the Rust one says "negative parts not cut: the cutter did not load" until
// `sf-cut` lands (#72). They are run and reported, and do NOT fail the run while they differ; the
// moment one is identical the run fails ("now identical: move it out of GAPS"), so the list can
// only shrink. Track A flips them to required in #72.
import { assertPinned, UPSTREAM_ROOT } from "../oracle/lib/pin.js";

assertPinned();
const RUST = Deno.realPathSync(Deno.args[0] ?? new URL("../../target/release/support-fins", import.meta.url).pathname);
const only = Deno.args[1] ?? "";
const JS_CLI = `${UPSTREAM_ROOT}/plugins/cli/support-fins.js`;
const MODELS = `${UPSTREAM_ROOT}/prototype/stress/models`;
const FIXTURES = `${UPSTREAM_ROOT}/tests/fixtures`;

const root = Deno.makeTempDirSync({ prefix: "sf-cli-parity-" });
const inputs = `${root}/inputs`;
Deno.mkdirSync(inputs);

// ---- the inputs --------------------------------------------------------------------------------
const models = [...Deno.readDirSync(MODELS)].filter((e) => e.name.endsWith(".stl")).map((e) => e.name).sort();
for (const m of models) Deno.copyFileSync(`${MODELS}/${m}`, `${inputs}/${m}`);
for (const f of ["bowl.stl", "dome_ceiling.stl", "torus_flat.stl"]) Deno.copyFileSync(`${FIXTURES}/${f}`, `${inputs}/${f}`);
for (const f of ["orca.3mf", "bambu.3mf", "prusaslicer3.3mf"]) Deno.copyFileSync(`${FIXTURES}/negative-parts/${f}`, `${inputs}/neg-${f}`);
{
  const { readSTL } = await import(`${UPSTREAM_ROOT}/web/stl.js`);
  const { writeThreeMF, writePlate3MF } = await import(`${UPSTREAM_ROOT}/web/threemf.js`);
  const tri = (name) => {
    const p = readSTL(Deno.readFileSync(`${MODELS}/${name}.stl`));
    const t = [];
    for (let i = 0; i < p.length; i += 3) t.push([p[i], p[i + 1], p[i + 2]]);
    return t;
  };
  const bytes = async (blob) => new Uint8Array(await blob.arrayBuffer());
  // a plain 3MF of one part, a plate of two models, and an OBJ of two objects
  Deno.writeFileSync(`${inputs}/one.3mf`, await bytes(writeThreeMF(tri("lbracket"), [], "one")));
  Deno.writeFileSync(`${inputs}/plate.3mf`, await bytes(writePlate3MF([
    { partTris: tri("lbracket"), finTris: [], name: "alpha" },
    { partTris: tri("wedge").map((v) => [v[0] + 80, v[1], v[2]]), finTris: [], name: "beta" },
  ], "plate", true)));
  const obj = (name) => {
    const t = tri(name);
    const lines = [`o ${name}`];
    t.forEach((v) => lines.push(`v ${v[0]} ${v[1]} ${v[2]}`));
    for (let i = 0; i < t.length; i += 3) lines.push(`f ${i + 1 - 0} ${i + 2} ${i + 3}`);
    return lines;
  };
  Deno.writeTextFileSync(`${inputs}/two.obj`, [...obj("cube"), ...obj("wedge")].join("\n") + "\n");
  Deno.writeTextFileSync(`${inputs}/cube.obj`, obj("cube").join("\n") + "\n");
  Deno.writeTextFileSync(`${inputs}/notamesh.txt`, "hello\n");
  Deno.writeTextFileSync(`${inputs}/empty.stl`, "");
}

// ---- project inputs (plugins/cli/tests/project.test.js's fixture) -----------------------------------
const { fixture } = await import("./project_fixtures.js");
const { ProjectZip } = await import(`${UPSTREAM_ROOT}/plugins/cli/project-zip.js`);
const { block } = await import(`${UPSTREAM_ROOT}/tests/_util.js`);
{
  const { readSTL } = await import(`${UPSTREAM_ROOT}/web/stl.js`);
  const lb = readSTL(Deno.readFileSync(`${MODELS}/lbracket.stl`));
  const sph = readSTL(Deno.readFileSync(`${MODELS}/sphere.stl`));
  const put = async (name, o) => Deno.writeFileSync(`${inputs}/${name}.3mf`, await fixture(o));
  await put("pj-cube", {});
  await put("pj-lb", { positions: lb, steep: true });
  await put("pj-sphere", { positions: sph });
  await put("pj-byobject", { positions: lb, steep: true, sequence: "by object" });
  await put("pj-plateseq", { positions: lb, steep: true, plateSequence: "by object" });
  await put("pj-mirror", { positions: lb, mirror: true });
  await put("pj-variable", { positions: lb, variable: 1 });
  await put("pj-nozzle08", { positions: lb, steep: true, nozzle: ["0.8"] });
  await put("pj-nozzle-mixed", { positions: lb, steep: true, nozzle: ["0.4", "0.8"] });
  await put("pj-duplicate", { duplicate: true });
  await put("pj-unprintable", { positions: lb, printable: false });
  await put("pj-emptyplates", { positions: lb, emptyPlates: [3, 4] });
  await put("pj-together", { positions: lb, together: true });
  await put("pj-samename", { positions: lb, sameName: true });
  await put("pj-orca", { positions: lb, app: "OrcaSlicer-2.3.0", layer: "0.2" });
  await put("pj-upright", { positions: lb, upright: true });
  await put("pj-dupplate", { positions: lb, plateIDs: [1, 1] });
  await put("pj-emptyplate", { positions: lb, plateIDs: ["", 5] });
  await put("pj-negative", { negative: true });
  await put("pj-cutters", { positions: lb, steep: true, cutters: [{ positions: block(2, 6, 2, 6, 2, 6) }] });
}

const INPUT_NAMES = new Set([...Deno.readDirSync(inputs)].map((e) => e.name));

// ---- the cases ---------------------------------------------------------------------------------
const cases = [];
const add = (id, args, files = []) => cases.push({ id, args, files });
const POSES = ["40,0,0", "0,35,20", "90,0,0"];
models.forEach((m, i) => {
  const b = m.replace(/\.stl$/, "");
  add(`${b}-default`, [m, "--rot", POSES[i % 3], "-o", "out.3mf"]);
  if (i % 2 === 0) add(`${b}-stl`, [m, "--rot", POSES[(i + 1) % 3], "-o", "out.stl"]);
  if (i % 3 === 0) add(`${b}-finsonly`, [m, "--fins-only", "-o", "fins.stl"]);
});
for (const m of ["lbracket", "wedge", "cube", "arch", "ramp", "torus"]) {
  const f = `${m}.stl`;
  add(`${m}-full`, [f, "--rot", "40,0,0", "--mode", "full", "-o", "o.3mf"]);
  add(`${m}-sway`, [f, "--rot", "0,0,0", "--sway", "--sway-grip-from", "5", "--sway-reach", "20", "-o", "o.3mf"]);
  add(`${m}-notines`, [f, "--rot", "40,0,0", "--no-tines", "-o", "o.stl"]);
  add(`${m}-vertical`, [f, "--rot", "40,0,0", "--tine-dir", "vertical", "--pin-depth", "0.3", "-o", "o.3mf"]);
  add(`${m}-nozzle`, [f, "--rot", "40,0,0", "--nozzle", "0.8", "--layer-height", "0.28", "-o", "o.3mf"]);
  add(`${m}-pad`, [f, "--rot", "40,0,0", "--bed-pad", "off", "--gap", "0.3", "-o", "o.3mf"]);
  add(`${m}-cutout`, [f, "--rot", "40,0,0", "--cutout", "arch", "--plate-only", "-o", "o.3mf"]);
  add(`${m}-threshold`, [f, "--rot", "20,10,0", "--threshold", "55", "--coverage", "80", "--tine-density", "30", "--json", "-o", "o.3mf"]);
  add(`${m}-fullsep`, [f, "--rot", "40,0,0", "--mode=full", "--separate-walls", "on", "--no-long-walls", "-o", "o.3mf"]);
}
for (const f of ["bowl.stl", "dome_ceiling.stl", "torus_flat.stl"]) add(`fixture-${f}`, [f, "--rot", "10,0,0", "-o", "o.3mf"]);
// the other file kinds
add("3mf-one", ["one.3mf", "-o", "o.3mf"]);
add("3mf-one-default-name", ["one.3mf", "--rot", "0,35,0"]);
add("3mf-plate-nopick", ["plate.3mf"]);
add("3mf-plate-1", ["plate.3mf", "--object", "1", "-o", "o.3mf"]);
add("3mf-plate-beta", ["plate.3mf", "--object=beta", "-o", "o.stl"]);
add("3mf-plate-missing", ["plate.3mf", "--object", "9"]);
add("obj-one", ["cube.obj", "--rot", "30,0,0", "-o", "o.3mf"]);
add("obj-two-nopick", ["two.obj"]);
add("obj-two-2", ["two.obj", "--object", "2", "-o", "o.stl"]);
add("many-inputs", ["lbracket.stl", "wedge.stl", "cube.stl", "--rot", "40,0,0", "--json"]);
add("many-with-failure", ["lbracket.stl", "notamesh.txt", "wedge.stl", "--rot", "40,0,0"]);
add("json-fins-only-none", ["cube.stl", "--fins-only", "--json"]);
// refusals and usage errors
for (const [id, args] of [
  ["help", ["--help"]], ["help-short", ["-h"]], ["version", ["--version"]], ["no-input", []],
  ["bad-option", ["lbracket.stl", "--bogus"]], ["bad-short", ["lbracket.stl", "-z"]], ["bad-neg", ["lbracket.stl", "--no-gap"]],
  ["needs-value", ["lbracket.stl", "--rot"]], ["bad-rot", ["lbracket.stl", "--rot", "1,2"]], ["bad-rot2", ["lbracket.stl", "--rot", "a,b,c"]],
  ["bad-number", ["lbracket.stl", "--gap", "x"]], ["range-gap", ["lbracket.stl", "--gap", "3"]], ["range-cov", ["lbracket.stl", "--coverage", "150"]],
  ["bad-choice", ["lbracket.stl", "--mode", "weird"]], ["bad-nozzle", ["lbracket.stl", "--nozzle", "0.3"]], ["nozzle-1", ["lbracket.stl", "--nozzle=1", "-o", "o.stl"]],
  ["bool-value", ["lbracket.stl", "--tines=yes"]], ["two-with-o", ["lbracket.stl", "wedge.stl", "-o", "x.stl"]],
  ["finsonly-rot", ["lbracket.stl", "--fins-only", "--rot", "1,2,3"]], ["finsonly-3mf", ["lbracket.stl", "--fins-only", "-o", "x.3mf"]],
  ["bad-ext", ["lbracket.stl", "-o", "x.txt"]], ["same-file", ["lbracket.stl", "-o", "lbracket.stl"]], ["same-file-dot", ["lbracket.stl", "-o", "./lbracket.stl"]],
  ["project-only-1", ["lbracket.stl", "--check"]], ["project-only-2", ["lbracket.stl", "--plate", "1"]], ["resume-alone", ["lbracket.stl", "--resume"]],
  ["cachedir-alone", ["lbracket.stl", "--cache-dir", "x"]], ["two-objects", ["lbracket.stl", "--object", "1", "--object", "2"]],
  ["invisible", ["lbracket.stl", "--rot", "40,0,0", "--tine-density", "50", "--no-tines", "-o", "o.stl"]],
  ["invisible2", ["lbracket.stl", "--rot", "40,0,0", "--coverage", "50", "--mode", "full", "-o", "o.stl"]],
  ["dashdash", ["--", "lbracket.stl", "-o", "o.stl"]],
  ["not-a-mesh", ["notamesh.txt"]], ["empty-stl", ["empty.stl"]], ["fin-the-fins", ["lbracket.stl", "--rot", "40,0,0", "-o", "fin.3mf", "+", "fin.3mf"]],
]) add(`err-${id}`, args);
// EXPECTED GAPS: a slicer's negative parts (the cut is `sf-cut`, #72)
const GAPS = new Set();
for (const f of ["orca", "bambu", "prusaslicer3"]) {
  for (const [tag, out] of [["3mf", "o.3mf"], ["stl", "o.stl"]]) {
    GAPS.add(`gap-${f}-${tag}`);
    add(`gap-${f}-${tag}`, [`neg-${f}.3mf`, "-o", out]);
  }
}
// project mode
const PJ = (id, input, flags, extra = {}) => add(`pj-${id}`, [input, "--preserve-project", ...flags], extra);
PJ("list-plates", "pj-lb.3mf", ["--list-plates"]);
PJ("list-plates-json", "pj-lb.3mf", ["--list-plates", "--json"]);
PJ("list-objects", "pj-lb.3mf", ["--list-objects", "--json"]);
PJ("list-objects-plate", "pj-together.3mf", ["--list-objects", "--plate", "1"]);
PJ("list-emptyplates", "pj-emptyplates.3mf", ["--list-plates"]);
PJ("list-unprintable", "pj-unprintable.3mf", ["--list-objects"]);
PJ("check", "pj-lb.3mf", ["--check"]);
PJ("check-json", "pj-lb.3mf", ["--check", "--json"]);
PJ("check-variable", "pj-variable.3mf", ["--check"]);
PJ("check-nozzle", "pj-nozzle-mixed.3mf", ["--check", "--json"]);
PJ("default", "pj-lb.3mf", []);
PJ("json", "pj-lb.3mf", ["--json"]);
PJ("cube-only-pad", "pj-cube.3mf", ["--json"]);
PJ("sphere-full", "pj-sphere.3mf", ["--mode", "full", "--json"]);
PJ("object-1", "pj-lb.3mf", ["--object", "1", "--json"]);
PJ("object-name", "pj-lb.3mf", ["--object", "Part 20", "--json"]);
PJ("object-missing", "pj-lb.3mf", ["--object", "7"]);
PJ("object-two", "pj-lb.3mf", ["--object", "1", "--object", "2", "--json"]);
PJ("plate-2", "pj-lb.3mf", ["--plate", "2", "--json"]);
PJ("plate-missing", "pj-lb.3mf", ["--plate", "9"]);
PJ("plate-dup", "pj-dupplate.3mf", ["--plate", "1"]);
PJ("plate-dup-plain", "pj-dupplate.3mf", ["--json"]);
PJ("plate-empty-plain", "pj-emptyplate.3mf", ["--json"]);
PJ("plate-empty-list", "pj-emptyplate.3mf", ["--list-plates"]);
PJ("plate-empty", "pj-emptyplates.3mf", ["--plate", "3"]);
PJ("fin-extruder", "pj-lb.3mf", ["--fin-extruder", "2", "--object", "1", "--json"]);
PJ("fin-extruder-bad", "pj-lb.3mf", ["--fin-extruder", "9", "--object", "1"]);
PJ("native-off", "pj-lb.3mf", ["--disable-native-supports", "--json"]);
PJ("nozzle-override", "pj-nozzle08.3mf", ["--nozzle", "0.4", "--json"]);
PJ("nozzle-project", "pj-nozzle08.3mf", ["--json"]);
PJ("nozzle-mixed", "pj-nozzle-mixed.3mf", ["--json"]);
PJ("layer-explicit", "pj-variable.3mf", ["--layer-height", "0.2", "--json"]);
PJ("variable", "pj-variable.3mf", []);
PJ("byobject", "pj-byobject.3mf", ["--json"]);
PJ("plateseq", "pj-plateseq.3mf", ["--json"]);
PJ("mirror", "pj-mirror.3mf", ["--json"]);
PJ("upright", "pj-upright.3mf", ["--json"]);
PJ("orca", "pj-orca.3mf", ["--json"]);
PJ("samename", "pj-samename.3mf", ["--object", "Part 10"]);
PJ("together", "pj-together.3mf", ["--json"]);
PJ("unprintable", "pj-unprintable.3mf", ["--json"]);
PJ("duplicate", "pj-duplicate.3mf", []);
PJ("negative-only-body", "pj-negative.3mf", []);
PJ("output", "pj-lb.3mf", ["-o", "custom.3mf", "--json"]);
PJ("output-not-3mf", "pj-lb.3mf", ["-o", "custom.stl"]);
PJ("output-same", "pj-lb.3mf", ["-o", "pj-lb.3mf"]);
PJ("not-3mf", "lbracket.stl", []);
PJ("not-bambu", "one.3mf", []);
PJ("with-rot", "pj-lb.3mf", ["--rot", "1,2,3"]);
PJ("mixed-inputs", "pj-lb.3mf", ["--json", "pj-cube.3mf"]);
add("pj-exists", ["pj-lb.3mf", "--preserve-project", "+", "pj-lb.3mf", "--preserve-project"]);
add("pj-overwrite", ["pj-lb.3mf", "--preserve-project", "+", "pj-lb.3mf", "--preserve-project", "--overwrite", "--json"]);
add("pj-batch-alias", ["pj-lb.3mf", "pj-cube.3mf", "--preserve-project", "-o", "pj-cube.3mf"]);
PJ("resume", "pj-lb.3mf", ["--resume", "--json"]);
PJ("resume-dir", "pj-lb.3mf", ["--resume", "--cache-dir", "mycache", "--json"]);
PJ("check-resume", "pj-lb.3mf", ["--check", "--resume"]);
// EXPECTED GAPS: a project with real cutters needs the cut (#72)
for (const id of ["cutters", "cutters-check"]) GAPS.add(`pj-${id}`);
PJ("cutters", "pj-cutters.3mf", ["--json"]);
PJ("cutters-check", "pj-cutters.3mf", ["--check", "--json"]);
// a missing file: only the prefix of the runtime's message is compared
add("err-missing-file", ["nope.stl", "--rot", "40,0,0"]);

// ---- running them ------------------------------------------------------------------------------
const dec = new TextDecoder();
async function exec(cmd, args, cwd) {
  const p = await new Deno.Command(cmd[0], { args: [...cmd.slice(1), ...args], cwd, stdout: "piped", stderr: "piped" }).output();
  return { code: p.code, out: dec.decode(p.stdout), err: dec.decode(p.stderr) };
}
function filesOf(dir) {
  const out = new Map();
  for (const e of Deno.readDirSync(dir)) if (e.isFile) out.set(e.name, Deno.readFileSync(`${dir}/${e.name}`));
  return out;
}
const UUID = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g;
/** `text` with each distinct UUID replaced by its order of first appearance. */
function normUuids(text) {
  const seen = new Map();
  return text.replace(UUID, (m) => { if (!seen.has(m)) seen.set(m, `UUID#${seen.size + 1}`); return seen.get(m); });
}
const tdec = new TextDecoder("utf-8", { fatal: true });
/** The run directory and a resume cache's key differ per run; they are not behaviour. */
const scrub = (text, dir) => text.split(dir).join("<DIR>").replace(/support-fins-[0-9a-f]{64}/g, "support-fins-<KEY>");
/** Project outputs: member by member, UUIDs normalised (see the header). */
async function compareProject(a, b) {
  const problems = [];
  let za, zb;
  try { za = new ProjectZip(a); zb = new ProjectZip(b); } catch (e) { return [`not both project archives: ${e.message}`]; }
  const na = [...za.entries.keys()], nb = [...zb.entries.keys()];
  if (JSON.stringify(na) !== JSON.stringify(nb)) problems.push(`members differ:\n    js ${JSON.stringify(na)}\n    rs ${JSON.stringify(nb)}`);
  if (!same(za.comment, zb.comment)) problems.push("archive comment differs");
  for (const name of na) {
    const ea = za.entries.get(name), eb = zb.entries.get(name);
    if (!eb) continue;
    const ra = za.bytes.subarray(ea.offset, ea.end), rb = zb.bytes.subarray(eb.offset, eb.end);
    if (same(ra, rb)) continue;
    // differs: only a UUID may
    let ta, tb;
    try { ta = tdec.decode(await za.read(name)); tb = tdec.decode(await zb.read(name)); } catch { problems.push(`member ${name}: bytes differ`); continue; }
    if (normUuids(ta) !== normUuids(tb)) problems.push(`member ${name}: differs beyond UUIDs`);
    else if (ra.length !== rb.length) problems.push(`member ${name}: same text, different stored size`);
  }
  return problems;
}
const same = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);

let failed = 0, ran = 0, gaps = 0;
for (const c of cases) {
  if (only && !c.id.includes(only)) continue;
  // "fin-the-fins": the second run reads the first's output
  const runs = (c.args.includes("+") ? [c.args.slice(0, c.args.indexOf("+")), c.args.slice(c.args.indexOf("+") + 1).map((a) => (a === "fin.3mf" ? "fin.3mf" : a))] : [c.args]);
  const dirs = {};
  const res = {};
  for (const [who, cmd] of [["js", ["deno", "run", "-A", JS_CLI]], ["rs", [RUST]]]) {
    const dir = `${root}/${c.id}-${who}`;
    Deno.mkdirSync(dir);
    for (const f of Deno.readDirSync(inputs)) Deno.copyFileSync(`${inputs}/${f.name}`, `${dir}/${f.name}`);
    dirs[who] = dir;
    let last = null;
    for (const a of runs) last = await exec(cmd, a, dir);
    res[who] = last;
  }
  ran++;
  const problems = [];
  const { js, rs } = res;
  if (js.code !== rs.code) problems.push(`exit ${js.code} vs ${rs.code}`);
  let jerr = js.err, rerr = rs.err;
  // an unreadable file: the runtime's own message (Deno's vs Rust's) is not compared, only that it was reported
  const unreadable = (e) => e.split("\n").map((l) => (l.includes(": No such file or directory") ? l.slice(0, l.indexOf(": No such file or directory")) : l)).join("\n");
  jerr = unreadable(jerr); rerr = unreadable(rerr);
  js.out = scrub(js.out, dirs.js); rs.out = scrub(rs.out, dirs.rs);
  jerr = scrub(jerr, dirs.js); rerr = scrub(rerr, dirs.rs);
  if (js.out !== rs.out) problems.push(`stdout:\n  js: ${JSON.stringify(js.out).slice(0, 600)}\n  rs: ${JSON.stringify(rs.out).slice(0, 600)}`);
  if (jerr !== rerr) problems.push(`stderr:\n  js: ${JSON.stringify(jerr).slice(0, 400)}\n  rs: ${JSON.stringify(rerr).slice(0, 400)}`);
  const jf = filesOf(dirs.js), rf = filesOf(dirs.rs);
  for (const name of new Set([...jf.keys(), ...rf.keys()])) {
    if (!jf.has(name)) problems.push(`file ${name}: only rust wrote it`);
    else if (!rf.has(name)) problems.push(`file ${name}: only js wrote it`);
    else if (!same(jf.get(name), rf.get(name))) {
      if (c.id.startsWith("pj-") && name.endsWith(".3mf") && !INPUT_NAMES.has(name)) {
        for (const pr of await compareProject(jf.get(name), rf.get(name))) problems.push(`file ${name}: ${pr}`);
      } else problems.push(`file ${name}: bytes differ (${jf.get(name).length} vs ${rf.get(name).length})`);
    }
  }
  if (GAPS.has(c.id)) {
    if (problems.length) { gaps++; console.log(`GAP  ${c.id} (expected until sf-cut, #72): ${problems[0].split("\n")[0]}`); }
    else { failed++; console.log(`FAIL ${c.id}: now identical: move it out of GAPS`); }
  } else if (problems.length) { failed++; console.log(`FAIL ${c.id} [${c.args.join(" ")}]\n  ${problems.join("\n  ")}`); }
  Deno.removeSync(dirs.js, { recursive: true });
  Deno.removeSync(dirs.rs, { recursive: true });
}
Deno.removeSync(root, { recursive: true });
console.log(`cli-parity: ${ran} cases, ${failed} failed, ${gaps} expected gaps`);
Deno.exit(failed ? 1 : 0);
