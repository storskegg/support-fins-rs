// Records parity/corpus/ffiexport/cases.json: what the page exports (web/ui/export.js) from a build, written by
// the page's own writers (web/stl.js writeBinarySTL, web/threemf.js writePlate3MF, prop.js splitInterface), so
// crates/sf-ffi/tests/export.rs can check `export(path, parts, builds, options_json)` writes the same bytes.
//
// export.js is DOM-bound, so its geometry and FORMATS closures are written out here line for line (the writers
// they call are the real ones): `toPrint` (modelGeometry), 'export-stl', 'export-3mf', 'export-fins'.
//
//   just oracle ffiexport
import { UPSTREAM_ROOT, assertPinned } from "./lib/pin.js";

assertPinned();
const WEB = `${UPSTREAM_ROOT}/web`;
const { buildTopology, analyze } = await import(`${WEB}/overhangs.js`);
const fins = await import(`${WEB}/fins.js`);
const { PROP, splitInterface } = await import(`${WEB}/prop.js`);
const { BRACE } = await import(`${WEB}/prop/brace.js`);
const { CUT } = await import(`${WEB}/cutout.js`);
const { readSTL, writeBinarySTL } = await import(`${WEB}/stl.js`);
const { readThreeMF, writePlate3MF } = await import(`${WEB}/threemf.js`);
const { cutNegatives } = await import(`${WEB}/cut.js`);
const { zipStore } = await import(`${WEB}/zip.js`);
const { FIN, PAD, buildFins } = fins;

const DEFAULTS = {
  roundFeet: FIN.roundFeet, padH: FIN.padH, padGrab: PAD.grab, padStyle: PAD.style, padCustom: { ...PAD.custom },
  propGap: PROP.gap, tallRibsHeight: BRACE.tallH, cutout: CUT.pattern, iface: PROP.iface,
  tineDir: PROP.tineDir, pinDepth: PROP.pinDepth, nozzle: 0.4, wallLines: 2,
};
const d2r = (d) => (d * Math.PI) / 180;
const rotX = (d) => { const c = Math.cos(d2r(d)), s = Math.sin(d2r(d)); return [1, 0, 0, 0, c, s, 0, -s, c]; };
const topoOf = (pos) => buildTopology({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });

function fnv64(bytes) {
  let h = 0xcbf29ce484222325n;
  for (const b of bytes) { h ^= BigInt(b); h = (h * 0x100000001b3n) & 0xffffffffffffffffn; }
  return h.toString(16).padStart(16, "0");
}
const bytesOf = async (blob) => new Uint8Array(await blob.arrayBuffer());

// The page's geometry for a posed, seated part (ui/export.js modelGeometry), with the supports it `added`.
function geometry({ pos, rot, offset, base, added, slicer }) {
  const toPrint = (flat) => {
    const out = new Array(flat.length / 3);
    for (let v = 0; v < out.length; v++) {
      const x = flat[v * 3], y = flat[v * 3 + 1], z = flat[v * 3 + 2];
      out[v] = [rot[0] * x + rot[3] * y + rot[6] * z + offset.x, rot[1] * x + rot[4] * y + rot[7] * z + offset.y, rot[2] * x + rot[5] * y + rot[8] * z + offset.z];
    }
    return out;
  };
  const partTris = toPrint(pos);
  const kept = slicer ? {
    partTris: toPrint(slicer.positions), colors: slicer.colors,
    extras: slicer.extras.map((e) => ({ kind: e.kind, name: e.name, settings: e.settings, tris: toPrint(e.positions) })),
  } : null;
  return { partTris, finTris: [...added], base, slicer: kept, colors: null };
}
// `placed(g, xy)` (ui/export.js): the model moved to its spot on the plate
function placed(g, [x, y]) {
  const at = (tris) => tris.map((p) => { const q = p.slice(); q[0] += x; q[1] += y; return q; });
  const slicer = g.slicer && { ...g.slicer, partTris: at(g.slicer.partTris), extras: g.slicer.extras.map((e) => ({ ...e, tris: at(e.tris) })) };
  return { ...g, partTris: at(g.partTris), finTris: at(g.finTris), slicer };
}
// FORMATS (ui/export.js), one model
const FORMATS = {
  stl: (g) => writeBinarySTL([...g.partTris, ...g.finTris], g.base),
  fins: (g) => writeBinarySTL(g.finTris, `${g.base} fins`),
  "3mf": (g, bed) => {
    const { body, iface } = splitInterface(g.finTris);
    const own = g.slicer ?? g;
    return writePlate3MF([{ partTris: own.partTris, finTris: body, name: g.base, iface, colors: own.colors, extras: g.slicer?.extras ?? [] }],
      { title: g.base, separate: true, bed: { x: bed[0], y: bed[1], z: 250 }, session: null });
  },
};

const out = [];
async function record(c, g) {
  const blob = c.kind === "3mf" ? FORMATS["3mf"](c.kind === "3mf" && c.placed ? placed(g, c.placed) : g, c.bed ?? [256, 256]) : FORMATS[c.kind](g);
  const bytes = await bytesOf(await blob);
  out.push({ name: c.name, part: c.part, rot: c.rot, dialog: c.dialog, tunables: c.tunables, kind: c.kind, options: c.options, bytes: bytes.length, hash: fnv64(bytes) });
  console.log(`${c.name}: ${c.kind} ${bytes.length} bytes`);
}

const STRESS = `${UPSTREAM_ROOT}/prototype/stress/models/`;
const FIXTURES = `${UPSTREAM_ROOT}/tests/fixtures/negative-parts/`;

async function cases() {
  const request = (tunables, extra = {}) => ({
    mode: "auto", bedPad: true, plateOnly: false, longWalls: true, separateWalls: false, tallRibs: true, tines: true,
    tineDensity: 1, layerHeight: 0.2, coverage: 0.5, tunables: { ...DEFAULTS, ...tunables }, ...extra,
  });
  const dialog = { tineDensity: 1, layerHeight: 0.2, coverage: 0.5, mode: "auto", tallRibs: true };

  for (const model of ["sphere", "lbracket"]) {
    const pos = readSTL(await Deno.readFile(`${STRESS}${model}.stl`));
    const rot = rotX(model === "sphere" ? 30 : 35);
    const iface = model === "lbracket" ? "all" : false;
    const tunables = { iface };
    const topo = topoOf(pos);
    const res = analyze(topo, 45, rot);
    const built = buildFins(topo, res, rot, request(tunables));
    const all = [...built.triangles, ...built.padTriangles];
    const g = geometry({ pos: topo.pos, rot, offset: res.offset, base: model, added: all });
    const common = { part: { model }, rot, dialog, tunables };
    await record({ ...common, name: `${model} posed, with fins`, kind: "stl", options: {} }, g);
    await record({ ...common, name: `${model} posed, fins only`, kind: "fins", options: { finsOnly: true } }, g);
    await record({ ...common, name: `${model} 3MF, bed 256, at 12,-7`, kind: "3mf", options: { bed: [256, 256], placed: [12, -7] }, bed: [256, 256], placed: [12, -7] }, g);
    // a fin removed by hand: the kept fins' triangles only (ui/remove.js filteredTriangles), then the pad
    const gone = built.fins[0].id;
    const kept = [];
    for (const f of built.fins) if (f.id !== gone) for (const [lo, hi] of f.triRanges) for (let t = lo; t < hi; t++) kept.push(built.triangles[t]);
    const gr = geometry({ pos: topo.pos, rot, offset: res.offset, base: model, added: [...kept, ...built.padTriangles] });
    await record({ ...common, name: `${model} fin ${gone} removed`, kind: "stl", options: { removed: [gone] } }, gr);
    await record({ ...common, name: `${model} fin ${gone} removed, 3MF`, kind: "3mf", options: { removed: [gone], bed: [256, 256] } }, gr);
    // fins off: the part alone, seated in the pose
    const g0 = geometry({ pos: topo.pos, rot, offset: res.offset, base: model, added: [] });
    const off = { fins: false, pose: rot, threshold: 45 };
    await record({ ...common, name: `${model} fins off, STL`, kind: "stl", options: off }, g0);
    await record({ ...common, name: `${model} fins off, 3MF`, kind: "3mf", options: { ...off, bed: [256, 256] }, bed: [256, 256] }, g0);
  }

  // a slicer's own file: negative parts cut at import, the paint and the parts written back
  const file = await Deno.readFile(`${FIXTURES}bambu.3mf`);
  const obj = (await readThreeMF(file)).objects[0];
  const cut = await cutNegatives(obj);
  const topo = topoOf(cut.positions);
  const rot = rotX(0);
  const res = analyze(topo, 45, rot);
  const built = buildFins(topo, res, rot, request({}));
  const g = geometry({ pos: topo.pos, rot, offset: res.offset, base: "bambu", added: [...built.triangles, ...built.padTriangles],
    slicer: obj.extras?.length ? { positions: obj.positions, colors: obj.colors ?? null, extras: obj.extras } : null });
  g.colors = cut.colors ?? null;
  const common = { part: { file: "tests/fixtures/negative-parts/bambu.3mf" }, rot, dialog, tunables: {} };
  await record({ ...common, name: "bambu.3mf, 3MF with its slicer parts and paint", kind: "3mf", options: { bed: [256, 256] }, bed: [256, 256] }, g);
  await record({ ...common, name: "bambu.3mf, STL", kind: "stl", options: {} }, g);
}

// Plates (ui/export.js plateGeometry + FORMATS['export-3mf'] over several models, ui/plates.js exportAll): several models
// on one plate, each where it stands, written by the real writePlate3MF; and the zip of two plates' 3MFs (zipStore).
const plates = [];
async function plateModel(spec) {
  const pos = readSTL(await Deno.readFile(`${STRESS}${spec.model}.stl`));
  const topo = topoOf(pos);
  const rot = rotX(spec.rotX);
  const res = analyze(topo, 45, rot);
  let added = [];
  if (spec.fins) {
    const request = { mode: "auto", bedPad: true, plateOnly: false, longWalls: true, separateWalls: false, tallRibs: true, tines: true,
      tineDensity: 1, layerHeight: 0.2, coverage: 0.5, tunables: { ...DEFAULTS, iface: spec.iface ?? false } };
    const built = buildFins(topo, res, rot, request);
    if (spec.removeFirst) {
      const gone = built.fins[0].id;
      for (const f of built.fins) if (f.id !== gone) for (const [lo, hi] of f.triRanges) for (let t = lo; t < hi; t++) added.push(built.triangles[t]);
      added.push(...built.padTriangles);
      spec.removed = [gone];
    } else added = [...built.triangles, ...built.padTriangles];
  }
  const g = placed(geometry({ pos: topo.pos, rot, offset: res.offset, base: spec.model, added }), spec.placed);
  const { body, iface } = splitInterface(g.finTris);
  return { model: { partTris: g.partTris, finTris: body, name: g.base, iface, colors: g.colors, extras: [] },
    json: { part: { model: spec.model }, rot, dialog: { tineDensity: 1, layerHeight: 0.2, coverage: 0.5, mode: "auto", tallRibs: true },
      tunables: { iface: spec.iface ?? false }, fins: spec.fins, removed: spec.removed ?? [], placed: spec.placed } };
}
async function plateCase(name, specs, bed) {
  const ms = [];
  for (const sp of specs) ms.push(await plateModel(sp));
  const blob = writePlate3MF(ms.map((m) => m.model), { title: ms[0].model.name, separate: true, bed: { x: bed[0], y: bed[1], z: 250 }, session: null });
  const bytes = await bytesOf(await blob);
  console.log(`plate ${name}: ${bytes.length} bytes`);
  return { name, bed, models: ms.map((m) => m.json), bytes: bytes.length, hash: fnv64(bytes), data: bytes };
}
{
  const a = await plateCase("sphere + lbracket", [
    { model: "sphere", rotX: 30, fins: true, placed: [-60, 10] },
    { model: "lbracket", rotX: 35, fins: true, iface: "all", placed: [55, -20] },
  ], [256, 256]);
  const b = await plateCase("three, one fin removed and one part alone", [
    { model: "lbracket", rotX: 35, fins: true, removeFirst: true, placed: [0, 0] },
    { model: "sphere", rotX: 30, fins: false, placed: [80.5, 12.25] },
    { model: "cube", rotX: 0, fins: false, placed: [-80, -40] },
  ], [250, 220]);
  const c = await plateCase("one model", [{ model: "sphere", rotX: 30, fins: true, placed: [3, -4] }], [256, 256]);
  for (const p of [a, b, c]) plates.push(p);
  const zip = zipStore([{ name: "plate-1-sphere-and-1-more-fins-0.4nozzle-0.2layer.3mf", data: a.data }, { name: "plate-2-sphere-fins-0.4nozzle-0.2layer.3mf", data: c.data }]);
  const zbytes = new Uint8Array(await zip.arrayBuffer ? await zip.arrayBuffer() : zip);
  plates.push({ name: "zip of two plates (both on a 256 bed)", zip: [{ name: "plate-1-sphere-and-1-more-fins-0.4nozzle-0.2layer.3mf", plate: 0 }, { name: "plate-2-sphere-fins-0.4nozzle-0.2layer.3mf", plate: 2 }],
    bytes: zbytes.length, hash: fnv64(zbytes) });
  for (const p of plates) delete p.data;
}

await cases();
Deno.writeTextFileSync(new URL("../corpus/ffiexport/cases.json", import.meta.url), JSON.stringify({ upstream: Deno.readTextFileSync(new URL("../../UPSTREAM", import.meta.url)).match(/commit=(\w+)/)[1], cases: out, plates }));
