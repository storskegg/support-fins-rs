// Reads 3MF files the NATIVE app wrote with the page's own session reader (web/ui/session.js: readThreeMF,
// partObjectOf, restoreSession -- run through parity/oracle/lib/sessionpage.js) and checks that they restore the state
// they were written from. crates/sf-ffi/tests/session.rs writes <dir>/<name>.3mf and <dir>/<name>.json:
//
//   { "beside": false, "session": "restored", "n": 4,
//     "state": { "walls": [{ "kind"?, "face"?, "capped", "a", "b"?, "m"? }], "removedSigs": [...], "finMode", "finsVisible", "form": {...} } }
//
// Walls are compared to 1e-5 (a session stores points to 1e-5 mm); everything else exactly, `form` as the set of
// fields the file names (the order is the writer's). Exit status 1 and a line per difference when any file differs.
//
//   deno run --allow-read --allow-write --allow-env --allow-run=git parity/cli/session_check.js <dir>
import { readBack } from "../oracle/lib/sessionpage.js";

const dir = Deno.args[0];
if (!dir) { console.error("usage: session_check.js <dir>"); Deno.exit(2); }

// fields the reader fills in for a file saved before they were kept (formOf's defaults): present whatever the file says
const EXTRA = new Set(["rounded-feet", "wall-lines", "long-walls", "tall-ribs", "curved-walls", "separate-walls", "sway-cross-height", "tall-ribs-height"]);
const near = (a, b) => a.length === b.length && a.every((v, k) => Math.abs(v - b[k]) <= 1.0000001e-5);
const failures = [];
let checked = 0;
for (const e of [...Deno.readDirSync(dir)].filter((e) => e.name.endsWith(".3mf")).sort((a, b) => a.name.localeCompare(b.name))) {
  const name = e.name.slice(0, -4);
  const want = JSON.parse(Deno.readTextFileSync(`${dir}/${name}.json`));
  const got = await readBack(Deno.readFileSync(`${dir}/${e.name}`), want.beside === true);
  const fail = (what, a, b) => failures.push(`${name}: ${what}: got ${JSON.stringify(a)}, want ${JSON.stringify(b)}`);
  if (got.session !== want.session) { fail("outcome", got.session, want.session); continue; }
  if (want.n !== undefined && got.n !== want.n) fail("restored count", got.n, want.n);
  if (want.note !== undefined && got.note !== want.note) fail("import note", got.note, want.note);
  const w = want.state, g = got.state;
  if (w && g) {
    if (g.walls.length !== w.walls.length) fail("walls", g.walls.length, w.walls.length);
    else {
      w.walls.forEach((x, k) => {
        const y = g.walls[k];
        if ((x.kind ?? "wall") !== (y.kind ?? "wall")) fail(`wall ${k} kind`, y.kind, x.kind);
        if (x.capped !== undefined && (x.capped === true) !== y.capped) fail(`wall ${k} capped`, y.capped, x.capped);
        if (x.face !== undefined && x.face !== y.face) fail(`wall ${k} face`, y.face, x.face);
        for (const key of ["a", "b", "m"]) {
          if (x[key] === undefined && y[key] === undefined) continue;
          if (!x[key] || !y[key] || !near(y[key], x[key])) fail(`wall ${k} ${key}`, y[key], x[key]);
        }
      });
    }
    if (JSON.stringify(g.removedSigs) !== JSON.stringify(w.removedSigs)) fail("removed", g.removedSigs, w.removedSigs);
    if (g.finMode !== w.finMode) fail("finMode", g.finMode, w.finMode);
    if (g.finsVisible !== w.finsVisible) fail("finsVisible", g.finsVisible, w.finsVisible);
    for (const [id, v] of Object.entries(w.form ?? {})) if (g.form[id] !== v) fail(`form ${id}`, g.form[id], v);
    if (w.form) for (const id of Object.keys(g.form)) if (!(id in w.form) && !EXTRA.has(id)) fail(`form ${id} (not written)`, g.form[id], undefined);
  }
  checked++;
}
for (const f of failures) console.error(f);
console.log(`${checked} native files read by the page's session reader, ${failures.length} differences`);
Deno.exit(failures.length ? 1 : 0);
