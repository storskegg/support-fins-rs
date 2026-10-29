// Records parity/corpus/ffidraw/cases.json: the hand-placed supports of web/ui/walls.js -- `sweepDrawn`
// (every drawn wall and sway brace re-stood for a pose), `braceAt` (the brace a click stands) and
// `arcPoints` (the band of a curve in progress) -- run from the REAL source text of walls.js.
//
// walls.js is a DOM module (three.js, the page's controls), so it cannot be imported in Deno. The oracle
// slices the functions out of the file at the pinned commit and compiles them next to the real engine
// modules (draw.js, drawarc.js, sway.js, fins/tall.js, ...), with small stand-ins for the page's state:
// the controls (`el`), the part's matrix (`toWorld`), `THREE.Vector3`. Nothing of walls.js is retyped here;
// a slice that is not found stops the run. `swayOpts()` is sliced from ui/finbuild.js the same way.
// crates/sf-ffi/tests/draw.rs checks the FFI (`DrawKit`) against the cases.
//
// A case: { name, part, rot, threshold, tunables, settings, auto, removed, walls, braces, bands, expected }
//   part      { model } or { block: [x0,x1,y0,y1,z0,z1] }
//   settings  what DrawKit takes as settings_json
//   auto      null, or { mode, dialog, tallRibs, sway? }: Auto's build (as ffibuild.js) the supports stand clear of
//   removed   Auto's removed fin ids
//   walls     [{ kind: "wall", a, b, m? } | { kind: "sway", a, face, capped }] in the part's own frame
//   braces    [{ face, p }]: braceAt probes (print space)
//   expected  { verts, hash, walls: [{ ok, ... }], braces: [{ ... }] }
//
//   just oracle ffidraw
import { UPSTREAM_ROOT, assertPinned } from "./lib/pin.js";

assertPinned();
const WEB = `${UPSTREAM_ROOT}/web`;
const { buildTopology, analyze } = await import(`${WEB}/overhangs.js`);
const fins = await import(`${WEB}/fins.js`);
const { PROP } = await import(`${WEB}/prop.js`);
const { BRACE } = await import(`${WEB}/prop/brace.js`);
const { CUT } = await import(`${WEB}/cutout.js`);
const { readSTL } = await import(`${WEB}/stl.js`);
const { FIN, PAD, buildFins, applyTunables } = fins;

// --- the sliced source ---------------------------------------------------------------------------------------
function slice(src, from, to) {
  const a = src.indexOf(from);
  const b = src.indexOf(to, a + from.length);
  if (a < 0 || b < 0) throw new Error(`walls.js slice not found: ${from} .. ${to}`);
  return src.slice(a, b);
}
const walls = Deno.readTextFileSync(`${WEB}/ui/walls.js`);
const finbuild = Deno.readTextFileSync(`${WEB}/ui/finbuild.js`);
const code = [
  slice(walls, "export function sweepDrawn(", "// ---- selecting a hand-placed support"),
  slice(walls, "function arcPoints(", "// A curve the ghost can't build"),
  slice(walls, "function autoSupports(", "// ---- sway brace preview"),
  slice(walls, "const SNAP_TOP", "/** Hovering an upright side"),
  slice(finbuild, "export function swayOpts()", "function makeFinWorker"),
].join("\n").replace("export function sweepDrawn", "function sweepDrawn").replace("export function swayOpts", "function swayOpts");

const dir = Deno.makeTempDirSync({ prefix: "sf-ffidraw-" });
Deno.writeTextFileSync(`${dir}/draw.mjs`, `
import { drawnWall } from '${WEB}/draw.js';
import { arcThrough, arcStations } from '${WEB}/drawarc.js';
import { planGrid } from '${WEB}/fins/tall.js';
import { swayAtFace, faceIsUpright } from '${WEB}/sway.js';
import { FIN } from '${WEB}/fins.js';
import { PROP } from '${WEB}/prop.js';
import { BRACE } from '${WEB}/prop/brace.js';

export class Vector3 {
  constructor(x = 0, y = 0, z = 0) { this.x = x; this.y = y; this.z = z; }
  copy(v) { this.x = v.x; this.y = v.y; this.z = v.z; return this; }
}
const THREE = { Vector3 };
const BAND_MAX = 129;
// The page's state the sliced functions read as free names; setPage() sets them per case.
let controls = {}, topology = null, lastResult = null, lastBuilt = null, drawnWalls = [], rotElements = null;
let removedIds = new Set();
const rotM3 = { get elements() { return rotElements; } };
const finsVisible = true;
const autoLike = () => true;
const el = (id) => {
  if (!controls[id]) throw new Error('control not stubbed: ' + id);
  return controls[id];
};
export function setPage(p) {
  controls = p.controls; topology = p.topology; lastResult = p.lastResult; lastBuilt = p.lastBuilt;
  drawnWalls = p.drawnWalls; rotElements = p.rot; removedIds = p.removed;
}
${code}
export const braceAtFor = (faceIndex, p) => braceAt(faceIndex, p);
export { sweepDrawn, arcPoints };
`);
const J = await import(`file://${dir}/draw.mjs`);

// --- models, poses, the auto build ---------------------------------------------------------------------------
const DEFAULTS = {
  roundFeet: FIN.roundFeet, padH: FIN.padH, padGrab: PAD.grab, padStyle: PAD.style, padCustom: { ...PAD.custom },
  propGap: PROP.gap, tallRibsHeight: BRACE.tallH, cutout: CUT.pattern, iface: PROP.iface,
  tineDir: PROP.tineDir, pinDepth: PROP.pinDepth, nozzle: 0.4, wallLines: 2,
};
const d2r = (d) => (d * Math.PI) / 180;
const rotX = (d) => { const c = Math.cos(d2r(d)), s = Math.sin(d2r(d)); return [1, 0, 0, 0, c, s, 0, -s, c]; };
const IDENT = [1, 0, 0, 0, 1, 0, 0, 0, 1];

function block(x0, x1, y0, y1, z0, z1) {
  const v = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
  const quad = (a, b, c, d) => [...a, ...b, ...c, ...a, ...c, ...d];
  return Float32Array.from([...quad(v[0], v[3], v[2], v[1]), ...quad(v[4], v[5], v[6], v[7]), ...quad(v[0], v[1], v[5], v[4]),
    ...quad(v[2], v[3], v[7], v[6]), ...quad(v[1], v[2], v[6], v[5]), ...quad(v[0], v[4], v[7], v[3])]);
}
const topoOf = (pos) => buildTopology({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });

function fnv64(words) {
  let h = 0xcbf29ce484222325n;
  const bytes = new Uint8Array(words.buffer, words.byteOffset, words.byteLength);
  for (const b of bytes) { h ^= BigInt(b); h = (h * 0x100000001b3n) & 0xffffffffffffffffn; }
  return h.toString(16).padStart(16, "0");
}
const hashVerts = (verts) => fnv64(Float32Array.from(verts.flatMap((v) => [v[0], v[1], v[2]])));

const toWorld = (rot, off) => (v, out) => {
  out.x = rot[0] * v.x + rot[3] * v.y + rot[6] * v.z + off.x;
  out.y = rot[1] * v.x + rot[4] * v.y + rot[7] * v.z + off.y;
  out.z = rot[2] * v.x + rot[5] * v.y + rot[8] * v.z + off.z;
  return out;
};
const toLocal = (rot, off, p) => {
  const d = [p[0] - off.x, p[1] - off.y, p[2] - off.z];
  return [rot[0] * d[0] + rot[1] * d[1] + rot[2] * d[2], rot[3] * d[0] + rot[4] * d[1] + rot[5] * d[2], rot[6] * d[0] + rot[7] * d[1] + rot[8] * d[2]];
};

/** Print-space centroids of the faces `pick` accepts, in face order. */
function centroids(topo, rot, off, pick) {
  const out = [];
  for (let f = 0; f < topo.nFaces; f++) {
    if (!pick(f)) continue;
    const p = [0, 0, 0];
    for (let k = 0; k < 3; k++) {
      const x = topo.pos[f * 9 + k * 3], y = topo.pos[f * 9 + k * 3 + 1], z = topo.pos[f * 9 + k * 3 + 2];
      p[0] += (rot[0] * x + rot[3] * y + rot[6] * z + off.x) / 3;
      p[1] += (rot[1] * x + rot[4] * y + rot[7] * z + off.y) / 3;
      p[2] += (rot[2] * x + rot[5] * y + rot[8] * z + off.z) / 3;
    }
    out.push({ f, p });
  }
  return out;
}
const dist2 = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]);

/** Pairs of overhang-face centroids a wall could join: spread over the part, deterministic. */
function pairs(cs, lo, hi, count) {
  const out = [];
  for (let i = 0; i < cs.length && out.length < count; i += Math.max(1, Math.floor(cs.length / (count * 3)))) {
    for (let j = i + 1; j < cs.length; j++) {
      const d = dist2(cs[i].p, cs[j].p);
      if (d >= lo && d <= hi) { out.push([cs[i], cs[j]]); break; }
    }
  }
  return out;
}

function autoBuild(c, topo, res, tunables) {
  const tallRibs = c.auto.tallRibs ?? true;
  const request = {
    mode: c.auto.mode, bedPad: true, plateOnly: false, longWalls: true, separateWalls: false,
    tallRibs, curvedWalls: undefined, tines: true, tineDensity: 1, layerHeight: 0.2,
    coverage: c.auto.mode === "full" ? FIN.coverDefault : 0.5,
    sway: c.auto.sway ? { on: true, roundFeet: tunables.roundFeet, uncappedDepth: c.auto.sway.uncappedDepth, gripFrom: c.auto.sway.gripFrom,
      tineSpacing: c.auto.sway.tineSpacing, reach: c.auto.sway.reach, crossRise: c.auto.sway.crossRise, gap: tunables.propGap, tines: true,
      iface: tunables.iface, layerHeight: 0.2, crossAbove: tallRibs ? tunables.tallRibsHeight : Infinity } : undefined,
    tunables,
  };
  return buildFins(topo, res, c.rot, request);
}

const CASES = [
  { name: "cube X35: two straight walls, tines on", part: { model: "cube" }, rot: rotX(35), tunables: {},
    settings: { tines: true, tineDensity: 1, layerHeight: 0.2, plateOnly: false, tallRibs: true },
    wallPairs: { count: 2, lo: 4, hi: 40 } },
  { name: "cube X35: tines off, plate only", part: { model: "cube" }, rot: rotX(35), tunables: {},
    settings: { tines: false, tineDensity: 1, layerHeight: 0.2, plateOnly: true, tallRibs: false },
    wallPairs: { count: 2, lo: 4, hi: 40 } },
  { name: "sphere X30: straight and curved walls, firm tines, short tall-rib height", part: { model: "sphere" }, rot: rotX(30),
    tunables: { tallRibsHeight: 8, wallLines: 4, roundFeet: true },
    settings: { tines: true, tineDensity: 0.6, layerHeight: 0.15, plateOnly: false, tallRibs: true },
    wallPairs: { count: 3, lo: 6, hi: 30 }, curves: 2 },
  { name: "sphere X30 with Auto's build on screen and a fin removed", part: { model: "sphere" }, rot: rotX(30),
    tunables: { tallRibsHeight: 8 },
    settings: { tines: true, tineDensity: 1, layerHeight: 0.2, plateOnly: false, tallRibs: true },
    auto: { mode: "auto", tallRibs: true }, removed: [0], wallPairs: { count: 3, lo: 6, hi: 30 } },
  { name: "lbracket X35, Auto with sway braces, a wall by hand", part: { model: "lbracket" }, rot: rotX(35),
    tunables: {}, settings: { tines: true, tineDensity: 1, layerHeight: 0.2, plateOnly: false, tallRibs: true },
    auto: { mode: "auto", tallRibs: true, sway: { gripFrom: 0, tineSpacing: 6, reach: 0.15, uncappedDepth: false, crossRise: 0.5 } },
    wallPairs: { count: 1, lo: 4, hi: 40 }, braces: 4 },
  { name: "a 300 mm post: hand braces, capped and full, cross ribs", part: { block: [-20, 20, -15, 15, 0, 300] }, rot: IDENT,
    tunables: { tallRibsHeight: 100, wallLines: 4 },
    settings: { tines: true, tineDensity: 1, layerHeight: 0.2, plateOnly: false, tallRibs: true,
      sway: { gripFrom: 0, tineSpacing: 6, reach: 0.3, uncappedDepth: true, crossRise: 0.3 } },
    braces: 6, braceWalls: true },
  { name: "the same post, tall ribs off, narrower braces", part: { block: [-20, 20, -15, 15, 0, 300] }, rot: IDENT,
    tunables: {}, settings: { tines: false, tineDensity: 1, layerHeight: 0.2, plateOnly: false, tallRibs: false,
      sway: { gripFrom: 20, tineSpacing: 8, reach: 0.1, uncappedDepth: false, crossRise: 0.5 } },
    braces: 5, braceWalls: true },
  { name: "lbracket X35: walls too short and a curve too tight are refused", part: { model: "lbracket" }, rot: rotX(35),
    tunables: {}, settings: { tines: true, tineDensity: 1, layerHeight: 0.2, plateOnly: false, tallRibs: true },
    refusals: true },
];

CASES.push(
  { name: "the post, nothing standing: braces previewed at four heights", part: { block: [-20, 20, -15, 15, 0, 300] }, rot: IDENT,
    tunables: { tallRibsHeight: 100 },
    settings: { tines: true, tineDensity: 1, layerHeight: 0.2, plateOnly: false, tallRibs: true,
      sway: { gripFrom: 0, tineSpacing: 6, reach: 0.15, uncappedDepth: false, crossRise: 0.5 } }, braces: 8 },
  { name: "lbracket X35 with Auto's braces: previews stay clear of them", part: { model: "lbracket" }, rot: rotX(35), tunables: {},
    settings: { tines: true, tineDensity: 1, layerHeight: 0.2, plateOnly: false, tallRibs: true },
    auto: { mode: "auto", tallRibs: true, sway: { gripFrom: 0, tineSpacing: 6, reach: 0.15, uncappedDepth: false, crossRise: 0.5 } },
    removed: [1], braces: 8 },
);

const SWAY_DEFAULT = { gripFrom: 0, tineSpacing: 6, reach: 0.15, uncappedDepth: false, crossRise: 0.5 };
const out = [];
for (const c of CASES) {
  const pos = c.part.model ? readSTL(Deno.readFileSync(`${UPSTREAM_ROOT}/prototype/stress/models/${c.part.model}.stl`)) : block(...c.part.block);
  const topo = topoOf(pos);
  const res = analyze(topo, 45, c.rot);
  const tunables = { ...DEFAULTS, ...c.tunables };   // the page always sends the full set
  const settings = { ...c.settings, sway: { ...SWAY_DEFAULT, ...(c.settings.sway ?? {}) } };
  applyTunables(tunables);
  const built = c.auto ? autoBuild(c, topo, res, tunables) : null;
  applyTunables(tunables);   // buildFins applies them too; the page's modules hold the same values either way
  const off = res.offset;

  // the clicks
  const over = centroids(topo, c.rot, off, (f) => res.kept[f]);
  const L = (p) => toLocal(c.rot, off, p);
  const walls = [];
  for (const [a, b] of pairs(over, c.wallPairs?.lo ?? 4, c.wallPairs?.hi ?? 40, c.wallPairs?.count ?? 0)) {
    walls.push({ kind: "wall", a: L(a.p), b: L(b.p) });
  }
  for (let i = 0; i < (c.curves ?? 0); i++) {
    const ps = pairs(over, 8, 26, 4)[i];
    if (!ps) continue;
    const mid = [(ps[0].p[0] + ps[1].p[0]) / 2, (ps[0].p[1] + ps[1].p[1]) / 2, (ps[0].p[2] + ps[1].p[2]) / 2];
    // out past the chord, perpendicular to it: a bulge
    const dx = ps[1].p[0] - ps[0].p[0], dy = ps[1].p[1] - ps[0].p[1], n = Math.hypot(dx, dy) || 1;
    const m = [mid[0] - (dy / n) * 7, mid[1] + (dx / n) * 7, mid[2]];
    walls.push({ kind: "wall", a: L(ps[0].p), b: L(ps[1].p), m: L(m) });
  }
  if (c.refusals) {
    const o = over[Math.floor(over.length / 2)].p;
    walls.push({ kind: "wall", a: L(o), b: L([o[0] + 0.5, o[1], o[2]]) });
    const q = over[Math.floor(over.length / 3)].p;
    walls.push({ kind: "wall", a: L([q[0], q[1], q[2]]), b: L([q[0] + 6, q[1], q[2]]), m: L([q[0] + 3, q[1] + 3, q[2]]) });   // radius 3
    walls.push({ kind: "wall", a: L([q[0], q[1], q[2]]), b: L([q[0] + 10, q[1], q[2]]), m: L([q[0] + 5, q[1] + 4, q[2]]) });
  }
  // hand braces: the faces that take one, spread over the part, each at a spot a quarter of the way up
  const probes = [];
  const takers = [];
  for (let f = 0; f < topo.nFaces; f++) {
    const nz = c.rot[2] * topo.nrm[f * 3] + c.rot[5] * topo.nrm[f * 3 + 1] + c.rot[8] * topo.nrm[f * 3 + 2];
    if (Math.abs(nz) < 0.2) takers.push(f);
  }
  const nb = c.braces ?? 0;
  for (let i = 0; i < nb && takers.length; i++) {
    const f = takers[Math.floor((i * takers.length) / nb)];
    const cp = centroids(topo, c.rot, off, (g) => g === f)[0].p;
    const z = [cp[2], cp[2] * 0.5 + 1, 40, 2][i % 4];
    probes.push({ face: f, p: [cp[0], cp[1], z] });
    if (c.braceWalls) walls.push({ kind: "sway", a: L([cp[0], cp[1], z]), face: f, capped: i % 2 === 1 });
  }
  if (c.braceWalls && probes.length) {   // a brace on a face that cannot take one
    const flat = Array.from({ length: topo.nFaces }, (_, f) => f).find((f) => {
      const nz = c.rot[2] * topo.nrm[f * 3] + c.rot[5] * topo.nrm[f * 3 + 1] + c.rot[8] * topo.nrm[f * 3 + 2];
      return nz > 0.9;
    });
    if (flat !== undefined) {
      const cp = centroids(topo, c.rot, off, (g) => g === flat)[0].p;
      walls.push({ kind: "sway", a: L(cp), face: flat, capped: false });
      probes.push({ face: flat, p: cp });
    }
  }

  // run the sliced walls.js
  const stand = walls.map((w) => ({ kind: w.kind === "sway" ? "sway" : undefined, a: new J.Vector3(...w.a), b: w.b ? new J.Vector3(...w.b) : undefined,
    m: w.m ? new J.Vector3(...w.m) : undefined, face: w.face, capped: w.capped ? true : undefined }));
  const ctl = (checked, v) => ({ checked, valueAsNumber: v });
  const sw = settings.sway;
  const controls = {
    tines: ctl(settings.tines), "tine-density": ctl(false, settings.tineDensity * 100),
    "layer-height": ctl(false, settings.layerHeight), "plate-only": ctl(settings.plateOnly), "tall-ribs": ctl(settings.tallRibs),
    "full-brace-depth": ctl(sw.uncappedDepth), "sway-from": ctl(false, sw.gripFrom), "sway-spacing": ctl(false, sw.tineSpacing),
    "sway-depth": ctl(false, sw.reach * 100), "sway-cross-height": ctl(false, sw.crossRise * 100), sway: ctl(true),
  };
  // the page divides the percent fields: keep the doubles exactly (reach, crossRise, tineDensity as the page computes them)
  settings.tineDensity = controls["tine-density"].valueAsNumber / 100;
  settings.sway.reach = controls["sway-depth"].valueAsNumber / 100;
  settings.sway.crossRise = Math.max(10, Math.min(90, controls["sway-cross-height"].valueAsNumber)) / 100;
  const removed = new Set(c.removed ?? []);
  J.setPage({ controls, topology: topo, lastResult: res, rot: c.rot, lastBuilt: built, removed, drawnWalls: stand });
  // printTriangles (walls.js), via the engine module the page uses for its own copy
  const tris = new Float64Array(topo.nFaces * 9);
  for (let i = 0; i < tris.length; i += 3) {
    const x = topo.pos[i], y = topo.pos[i + 1], z = topo.pos[i + 2], r = c.rot;
    tris[i] = r[0] * x + r[3] * y + r[6] * z + off.x;
    tris[i + 1] = r[1] * x + r[4] * y + r[7] * z + off.y;
    tris[i + 2] = r[2] * x + r[5] * y + r[8] * z + off.z;
  }
  const swept = J.sweepDrawn({ topology: topo, res, rot: c.rot, tris, walls: stand, toWorld: toWorld(c.rot, off),
    auto: built, gone: removed });
  const wallOut = stand.map((w) => {
    const i = w.info ?? {};
    return { ok: !!w.ok, triStart: w.triStart, triEnd: w.triEnd, kind: w.kind === "sway" ? "sway" : "wall",
      reason: w.ok ? "" : String(i.reason ?? ""), length: i.length ?? 0, height: i.height ?? 0, tines: i.tines ?? 0,
      ribs: w.kind === "sway" ? 0 : i.braces ?? 0, cross: w.kind === "sway" ? i.cross ?? 0 : 0, stilt: w.kind === "sway" ? i.stilt ?? 0 : 0 };
  });
  const braceOut = probes.map((q) => {
    const b = J.braceAtFor(q.face, q.p);
    const r = b.r;
    return { ok: !!r.ok, reason: r.ok ? "" : String(r.reason ?? ""), full: b.full, point: b.point.slice(), key: b.key,
      height: r.ok ? r.height : 0, verts: r.ok ? r.tris.length : 0, hash: r.ok ? hashVerts(r.tris) : "" };
  });
  out.push({ name: c.name, part: c.part, rot: c.rot, threshold: 45, tunables, settings, auto: c.auto ?? null, removed: c.removed ?? [],
    walls, braces: probes, expected: { verts: swept.length, hash: hashVerts(swept), walls: wallOut, braces: braceOut } });
  console.log(`${c.name}: ${walls.length} supports, ${wallOut.filter((w) => w.ok).length} built, ${swept.length} verts, ${probes.length} probes`);
}

// the band of a curve in progress
const bands = [
  [[0, 0, 0], [20, 0, 0], [10, 8, 0]], [[0, 0, 2], [30, 5, 4], [14, -12, 3]], [[0, 0, 0], [10, 0, 0], [5, 0.0000001, 0]],
  [[-5, 3, 1], [-5, 3.5, 1], [4, 9, 2]], [[0, 0, 0], [40, 0, 0], [20, 60, 0]],
].map(([a, b, m]) => ({ a, b, m, expected: J.arcPoints(new J.Vector3(...a), new J.Vector3(...b), new J.Vector3(...m)).map((p) => [p.x, p.y, p.z]) }));

Deno.mkdirSync(new URL("../corpus/ffidraw/", import.meta.url), { recursive: true });
Deno.writeTextFileSync(new URL("../corpus/ffidraw/cases.json", import.meta.url), JSON.stringify({
  upstream: Deno.readTextFileSync(new URL("../../UPSTREAM", import.meta.url)).match(/commit=(\w+)/)[1], cases: out, bands }));
Deno.removeSync(dir, { recursive: true });
