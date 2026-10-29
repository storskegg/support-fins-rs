// Records parity/corpus/session/: sessions (web/ui/session.js, #214) saved and read back by the page's own code.
//
//   cases.json     one entry per case: the state the web saved, the 3MF it wrote, the state its reader restored
//                  from that file, and the import note it showed.
//   <slug>.3mf     the files, as the page's writer (web/threemf.js writePlate3MF) wrote them.
//
// session.js is a DOM module, so it cannot be imported in Deno as is. The oracle runs the REAL source text:
// a copy of ui/session.js in a temp directory whose only change is its import specifiers, which point at small
// stand-ins for the page's state (the part and its topology, the drawn walls, the settings' fields, history.js's
// `restoreState`, which records what a restore asks for). The fields are the page's own: index.html is parsed for
// every <input> and <select>, so `settable` and `readable` see the real option lists and types. `FORM_IDS` and
// `VIEWER_IDS` are read off history.js. `renderImportNote` is sliced out of ui/io.js (English `t` / `tn`).
// What is written is the real `sessionOf` over the real `writePlate3MF`; what is read is the real `readThreeMF`,
// `partObjectOf` and `restoreSession`. Nothing of the page is retyped but `toPrint` (ui/export.js, sliced) and
// setPart's centring (ui/part.js:81-90).
//
// A case: { name, file, beside, read, write? }
//   read   { session: "none" | "restored" | "changed" | "unreadable", n, centre, state, note, noteScaled }
//          state = restoreState's argument less the fixed bits: { walls, removedSigs, finMode, finsVisible, form }
//          (form: the page's fields only -- a key naming another element is inert in restoreForm)
//          note = renderImportNote after the load, noteScaled = after forgetSessionNote (ui/scale.js)
//   write  how the file was made, for crates/sf-ffi/tests/session.rs to write it again:
//          { model, scale, rot, threshold, fins: {mode, removeFirst, iface} | null, placed, state, session, bytes, hash }
//          state = the web's model as sessionOf took it, its walls in the web's local (centred) frame
//
//   just oracle session
import { UPSTREAM_ROOT, WEB, assertPinned } from "./lib/pin.js";
import { THREE, FORM_IDS, VIEWER_IDS, WEBDIR, session, setPart, toPrintOf, readBack, defaultForm, elements } from "./lib/sessionpage.js";

assertPinned();
const { analyze } = await import(`${WEB}overhangs.js`);
const fins = await import(`${WEB}fins.js`);
const { PROP, splitInterface } = await import(`${WEB}prop.js`);
const { BRACE } = await import(`${WEB}prop/brace.js`);
const { CUT } = await import(`${WEB}cutout.js`);
const { readSTL } = await import(`${WEB}stl.js`);
const { writePlate3MF } = await import(`${WEB}threemf.js`);
const { scaleAbout } = await import(`${WEB}scale.js`);
const { assignKeys } = await import(`${WEB}ui/finkeys.js`);
const { FIN, PAD, buildFins } = fins;

const OUT = new URL("../corpus/session/", import.meta.url);
Deno.mkdirSync(OUT, { recursive: true });
const STRESS = `${UPSTREAM_ROOT}/prototype/stress/models/`;

// ---------------------------------------------------------------------------------------------- helpers
function fnv64(bytes) {
  let h = 0xcbf29ce484222325n;
  for (const x of bytes) { h ^= BigInt(x); h = (h * 0x100000001b3n) & 0xffffffffffffffffn; }
  return h.toString(16).padStart(16, "0");
}
const bytesOf = async (blob) => new Uint8Array(await blob.arrayBuffer());
const slug = (s) => s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");

const rotOf = (axisDegs) => {
  const q = new THREE.Quaternion();
  for (const [axis, deg] of axisDegs) {
    q.premultiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(...axis), (deg * Math.PI) / 180));
  }
  return new THREE.Matrix3().setFromMatrix4(new THREE.Matrix4().makeRotationFromQuaternion(q)).elements.slice();
};

const DEFAULTS = {
  roundFeet: FIN.roundFeet, padH: FIN.padH, padGrab: PAD.grab, padStyle: PAD.style, padCustom: { ...PAD.custom },
  propGap: PROP.gap, tallRibsHeight: BRACE.tallH, cutout: CUT.pattern, iface: PROP.iface,
  tineDir: PROP.tineDir, pinDepth: PROP.pinDepth, nozzle: 0.4, wallLines: 2,
};

// ---------------------------------------------------------------------------------------------- the cases
const cases = [];
const readOnly = [];

/**
 * Save a model as the page does (ui/export.js 'export-3mf' for one model) and read the file back.
 * spec: { name, model, scale?, rot, fins?: { mode, removeFirst?, iface? }, placed?, state: { finMode, finsVisible, form, walls, removed } }
 * `state.walls`: [{ kind?, a, b?, m?, capped? }] in the part's local (centred) frame, or { face, kind: 'wall'|'sway', ... } by face centroid.
 */
async function save(spec) {
  const pos0 = readSTL(await Deno.readFile(`${STRESS}${spec.model}.stl`));
  let file = pos0;
  if (spec.scale) {
    // setScale (ui/scale.js): the centred base scaled about the file's centre
    const g0 = setPart(pos0);
    file = scaleAbout(g0.geometry.getAttribute("position").array.slice(), spec.scale, [0, 0, 0], g0.centre);
  }
  const { topo } = setPart(file);
  const rot = spec.rot;
  const res = analyze(topo, 45, rot);
  const walls = (spec.state.walls ?? []).map((w) => {
    const at = w.face !== undefined ? faceCentre(topo, w.face) : w.a;
    return { ...(w.kind === "sway" ? { kind: "sway", capped: w.capped === true } : {}), a: at, ...(w.kind === "sway" ? {} : { b: w.b, ...(w.m ? { m: w.m } : {}) }) };
  });
  // the fins (ui/export.js: the build's triangles, the pad), and the fin ids a removal names
  let added = [], removed = spec.state.removed ?? [];
  if (spec.fins) {
    const request = { mode: spec.fins.mode, bedPad: false, plateOnly: false, longWalls: true, separateWalls: false, tallRibs: true, tines: false,
      tineDensity: 0, layerHeight: 0.2, coverage: 0.5, tunables: { ...DEFAULTS, iface: spec.fins.iface ?? false } };
    const built = buildFins(topo, res, rot, request);
    added = [...built.triangles, ...built.padTriangles];
    if (spec.fins.removeFirst) {
      const records = assignKeys(built.fins.map((f) => ({ ...f })));
      removed = records.slice(0, spec.fins.removeFirst).map((r) => r.sig);
    }
  }
  const toPrint = toPrintOf(rot, res.offset.x, res.offset.y, res.offset.z);
  const partTris = toPrint(topo.pos);
  const form = spec.state.form ?? defaultForm();
  const model = { rot, offset: res.offset, finMode: spec.state.finMode, finsVisible: spec.state.finsVisible, form,
    walls: walls.map((w) => ({ ...w, a: new THREE.Vector3(...w.a), ...(w.b ? { b: new THREE.Vector3(...w.b) } : {}), ...(w.m ? { m: new THREE.Vector3(...w.m) } : {}) })),
    removed: new Set(removed) };
  const saved = session.sessionOf(partTris, model);
  const at = spec.placed ?? [0, 0];
  const shifted = partTris.map((p) => [p[0] + at[0], p[1] + at[1], p[2]]);
  const fin = added.map((p) => [p[0] + at[0], p[1] + at[1], p[2]]);
  const { body, iface } = splitInterface(fin);
  const blob = writePlate3MF([{ partTris: shifted, finTris: body, name: spec.model, iface, colors: null, extras: [] }],
    { title: spec.model, separate: true, bed: { x: 256, y: 256, z: 250 }, session: saved });
  const bytes = await bytesOf(blob);
  const file3mf = `${slug(spec.name)}.3mf`;
  Deno.writeFileSync(new URL(file3mf, OUT), bytes);
  const read = await readBack(bytes, false);
  const readBeside = spec.beside ? await readBack(bytes, true) : undefined;
  cases.push({
    name: spec.name, file: file3mf, read, ...(readBeside ? { readBeside } : {}),
    write: {
      model: spec.model, scale: spec.scale ?? null, rot, threshold: 45,
      fins: spec.fins ? { mode: spec.fins.mode, iface: spec.fins.iface ?? false } : null, placed: at,
      state: { finMode: spec.state.finMode, finsVisible: spec.state.finsVisible, form,
        walls: walls.map((w) => ({ ...(w.kind ? { kind: w.kind } : {}), ...(w.capped ? { capped: true } : {}), a: w.a, ...(w.b ? { b: w.b } : {}), ...(w.m ? { m: w.m } : {}) })),
        removed },
      session: JSON.stringify(saved), bytes: bytes.length, hash: fnv64(bytes),
    },
  });
  console.log(`${spec.name}: ${bytes.length} bytes, ${read.session}${read.n !== undefined ? ` (${read.n} walls)` : ""}`);
}

/** The centre of face `f` of a topology (the local frame). */
function faceCentre(topo, f) {
  const p = topo.pos, o = f * 9;
  return [0, 1, 2].map((k) => (p[o + k] + p[o + 3 + k] + p[o + 6 + k]) / 3);
}

/** A hand-built session wrapped in a real file of `model` (what a hand-edited or older file looks like). */
async function edited(name, model, sessionObject, { beside = false, scale } = {}) {
  const pos = readSTL(await Deno.readFile(`${STRESS}${model}.stl`));
  const { topo } = setPart(pos);
  const toPrint = toPrintOf(rotOf([]), 0, 0, 0);
  const tris = toPrint(topo.pos);
  const blob = writePlate3MF([{ partTris: tris, finTris: [], name: model, iface: [], colors: null, extras: [] }],
    { title: model, separate: true, bed: { x: 256, y: 256, z: 250 }, session: sessionObject });
  const bytes = await bytesOf(blob);
  const file = `${slug(name)}.3mf`;
  Deno.writeFileSync(new URL(file, OUT), bytes);
  const read = await readBack(bytes, beside);
  readOnly.push(name);
  cases.push({ name, file, beside, session: sessionObject, read });
  console.log(`${name}: ${read.session}`);
}

// geometry of lbracket (the centred frame): which faces to stand things on is read off its topology
const lb = setPart(readSTL(await Deno.readFile(`${STRESS}lbracket.stl`))).topo;
const ush = setPart(readSTL(await Deno.readFile(`${STRESS}ushape.stl`))).topo;
console.log(`lbracket ${lb.nFaces} faces, ushape ${ush.nFaces} faces`);

const form = defaultForm();
const mix = (o) => ({ ...form, ...o });
const IDENT = rotOf([]);
const POSED = rotOf([[[1, 0, 0], 35], [[0, 0, 1], 20]]);

await save({ name: "one model, default settings", model: "lbracket", rot: IDENT, state: { finMode: "auto", finsVisible: true } });
await save({ name: "one model, fins hidden", model: "lbracket", rot: IDENT, state: { finMode: "auto", finsVisible: false } });
await save({ name: "non-default pose, auto fins", model: "lbracket", rot: POSED, fins: { mode: "auto" }, state: { finMode: "auto", finsVisible: true } });
await save({ name: "auto fins, two removed", model: "lbracket", rot: rotOf([[[1, 0, 0], 35]]), fins: { mode: "auto", removeFirst: 2 }, beside: true,
  state: { finMode: "auto", finsVisible: true } });
await save({ name: "full coverage, one removed", model: "lbracket", rot: rotOf([[[1, 0, 0], 35]]), fins: { mode: "full", removeFirst: 1 },
  state: { finMode: "full", finsVisible: true } });
await save({ name: "draw mode, walls and braces", model: "ushape", rot: IDENT, beside: true,
  state: { finMode: "draw", finsVisible: true, walls: [
    { a: faceCentre(ush, 0), b: faceCentre(ush, 3) },
    { a: [-8.25, 4.5, -3], b: [6.125, -2.75, -3], m: [0.5, 5.25, -3] },
    { kind: "sway", face: 2, capped: true },
    { kind: "sway", face: 5 },
  ] } });
await save({ name: "draw mode, posed", model: "lbracket", rot: POSED,
  state: { finMode: "draw", finsVisible: true, walls: [{ a: [1.5, 2.5, 3.5], b: [-4, 0.25, 2] }, { kind: "sway", face: 1, capped: true }] } });
await save({ name: "scaled part", model: "lbracket", scale: 1.5, rot: IDENT,
  state: { finMode: "draw", finsVisible: true, walls: [{ a: [3, 1, -2], b: [-3, 1, -2] }] } });
await save({ name: "interface and printer settings", model: "lbracket", rot: rotOf([[[0, 1, 0], 25]]), fins: { mode: "auto", iface: "flat" },
  state: { finMode: "auto", finsVisible: true, form: mix({
    thr: "50", tines: false, "tine-density": "35", "tine-dir": "vertical", "pin-depth": "0.2", "layer-height": "0.16", gap: "0.25",
    "bed-pad": "custom", "pad-h": "0.3", "pad-gap": "0.15", "pad-grip": "0.05", "pad-margin": "6.5", sway: true, "sway-from": "20",
    "sway-spacing": "8", "sway-depth": "20", "sway-cross-height": "60", "full-brace-depth": true, "rounded-feet": true, "wall-lines": "4",
    cutout: "arch", iface: "flat", coverage: "75", "plate-only": true, "long-walls": false, "separate-walls": true, "tall-ribs": false,
    "tall-ribs-height": "12.5", "curved-walls": true }) } });
await save({ name: "placed away from the centre", model: "cube", rot: IDENT, placed: [37.5, -22.25],
  state: { finMode: "auto", finsVisible: true } });

// several models on a plate carry no session (ui/export.js: `gs.length === 1 ? gs[0].session() : null`)
{
  const parts = [];
  for (const [model, at] of [["lbracket", [-40, 0]], ["cube", [40, 10]]]) {
    const { topo } = setPart(readSTL(await Deno.readFile(`${STRESS}${model}.stl`)));
    const res = analyze(topo, 45, IDENT);
    const toPrint = toPrintOf(IDENT, res.offset.x, res.offset.y, res.offset.z);
    parts.push({ partTris: toPrint(topo.pos).map((p) => [p[0] + at[0], p[1] + at[1], p[2]]), finTris: [], name: model, iface: [], colors: null, extras: [] });
  }
  const bytes = await bytesOf(writePlate3MF(parts, { title: "lbracket", separate: true, bed: { x: 256, y: 256, z: 250 }, session: null }));
  Deno.writeFileSync(new URL("two-models-no-session.3mf", OUT), bytes);
  cases.push({ name: "two models on a plate", file: "two-models-no-session.3mf", read: await readBack(bytes, false) });
}

// hand-built and older sessions
const size = [...(await (async () => {
  const { geometry } = setPart(readSTL(await Deno.readFile(`${STRESS}lbracket.stl`)));
  const bb = geometry.boundingBox;
  return [bb.max.x - bb.min.x, bb.max.y - bb.min.y, bb.max.z - bb.min.z].map((v) => Math.round(v * 1e5) / 1e5);
})())];
const base = { v: 1, part: { size } };
await edited("session without a form or a mode", "lbracket", { ...base, walls: [{ a: [1, 2, 3], b: [4, 5, 6] }] });
await edited("session with an empty form", "lbracket", { ...base, form: {}, finMode: "full" });
await edited("form with values a field cannot take", "lbracket", { ...base, finsVisible: false, form: {
  thr: "abc", tines: "yes", "tine-density": "37", "tine-dir": "diagonal", "wall-lines": "5", cutout: "arch", gap: " ", "pad-h": "0.25",
  nozzle: "0.8", "highlight-small": false, export: "x", bogus: true, "layer-height": "0.12", "tall-ribs": true, sway: 1 } });
await edited("form of an older file", "lbracket", { ...base, finMode: "draw", form: { gap: "0.3", tines: true, "sway-cross-height": "100", "tall-ribs-height": "400" } });
await edited("form that is not an object", "lbracket", { ...base, form: "none" });
await edited("form that is an array", "lbracket", { ...base, form: ["a"] });
await edited("part changed since it was exported", "lbracket", { ...base, part: { size: [size[0] + 0.2, size[1], size[2]] }, walls: [{ a: [1, 2, 3], b: [4, 5, 6] }] });
await edited("part within the size tolerance", "lbracket", { ...base, part: { size: [size[0] + 0.04, size[1] - 0.05, size[2]] }, walls: [{ a: [1, 2, 3], b: [4, 5, 6] }] });
await edited("session without a size", "lbracket", { v: 1, walls: [] });
await edited("unknown fin mode", "lbracket", { ...base, finMode: "combined" });
await edited("fins visible that is not a boolean", "lbracket", { ...base, finsVisible: 1 });
await edited("removed that is not a list of text", "lbracket", { ...base, removed: [1, 2] });
await edited("a wall with one point", "lbracket", { ...base, walls: [{ a: [1, 2, 3] }] });
await edited("a wall with a bad middle point", "lbracket", { ...base, walls: [{ a: [1, 2, 3], b: [1, 2, 3], m: [1, 2] }] });
await edited("a wall of an unknown kind", "lbracket", { ...base, walls: [{ kind: "post", a: [1, 2, 3] }] });
await edited("walls null and removed listed", "lbracket", { ...base, walls: null, removed: ["wedge:1.5,2,3:4:0.5", "prop:0,0,0:1:0"] });
await edited("a brace saved before the height cap", "ushape", { ...base, part: { size: await (async () => {
  const { geometry } = setPart(readSTL(await Deno.readFile(`${STRESS}ushape.stl`)));
  const bb = geometry.boundingBox;
  return [bb.max.x - bb.min.x, bb.max.y - bb.min.y, bb.max.z - bb.min.z].map((v) => Math.round(v * 1e5) / 1e5);
})() }, walls: [{ kind: "sway", a: faceCentre(ush, 4) }, { kind: "sway", a: faceCentre(ush, 1), capped: false }, { kind: "sway", a: faceCentre(ush, 2), capped: 1 }] });
await edited("session of another version", "lbracket", { v: 2, part: { size } });

// the table of fields the oracle read off index.html, for crates/sf-io's FORM_FIELDS to be checked against
const fields = FORM_IDS.map((id) => {
  const f = elements.get(id);
  return { id, type: f.type, options: f.options.map((o) => o.value), min: f.min ?? null, max: f.max ?? null, step: f.step ?? null,
    default: f.type === "checkbox" ? f.checked : f.value ?? null, viewer: VIEWER_IDS.includes(id) };
});

Deno.writeTextFileSync(new URL("cases.json", OUT), JSON.stringify({
  upstream: Deno.readTextFileSync(new URL("../../UPSTREAM", import.meta.url)).match(/commit=(\w+)/)[1],
  formIds: FORM_IDS, viewerIds: VIEWER_IDS, fields, cases,
}, null, 1));
console.log(`${cases.length} cases`);
