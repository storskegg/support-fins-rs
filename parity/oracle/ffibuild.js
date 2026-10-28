// Records parity/corpus/ffibuild/cases.json: builds through the page's own request path -- the
// `finOpts()`-shaped request ui/finbuild.js sends, run by `buildFins` exactly as web/finworker.js runs it
// -- with the `tunables` object the page attaches. crates/sf-ffi/tests/web_build.rs checks that
// `Builder.build(rot, opts_json, tunables_json)` gives the same numbers, records and triangles.
//
// A case: { name, part, rot, dialog, tunables, expected }.
//   part      { model } (a prototype/stress model) or { block: [x0,x1,y0,y1,z0,z1] }
//   rot       the pose, 9 numbers column-major (the page's rotM3 elements)
//   dialog    what a host sends as opts_json: options.json keys + the page's request-level extras
//             (tallRibs, curvedWalls, sway.uncappedDepth, sway.crossRise)
//   tunables  what a host sends as tunables_json: finOpts().tunables, the full set
//   expected  a summary of the JS `built`: counts, an FNV-1a 64 hash of the fin then pad triangles as
//             little-endian Float32 words, and the per-fin / per-wall records
//
//   just oracle ffibuild
import { UPSTREAM_ROOT, assertPinned } from "./lib/pin.js";

assertPinned();
const WEB = `${UPSTREAM_ROOT}/web`;
const { buildTopology, analyze } = await import(`${WEB}/overhangs.js`);
const fins = await import(`${WEB}/fins.js`);
const { PROP } = await import(`${WEB}/prop.js`);
const { BRACE } = await import(`${WEB}/prop/brace.js`);
const { CUT } = await import(`${WEB}/cutout.js`);
const { readSTL } = await import(`${WEB}/stl.js`);
const { FIN, PAD, buildFins } = fins;

// the page's defaults, read before any build touches the module state
const DEFAULTS = {
  roundFeet: FIN.roundFeet, padH: FIN.padH, padGrab: PAD.grab, padStyle: PAD.style, padCustom: { ...PAD.custom },
  propGap: PROP.gap, tallRibsHeight: BRACE.tallH, cutout: CUT.pattern, iface: PROP.iface,
  tineDir: PROP.tineDir, pinDepth: PROP.pinDepth, nozzle: 0.4, wallLines: 2,
};

const d2r = (d) => (d * Math.PI) / 180;
const rotX = (d) => { const c = Math.cos(d2r(d)), s = Math.sin(d2r(d)); return [1, 0, 0, 0, c, s, 0, -s, c]; };

function block(x0, x1, y0, y1, z0, z1) {
  const v = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
  const quad = (a, b, c, d) => [...a, ...b, ...c, ...a, ...c, ...d];
  return Float32Array.from([...quad(v[0], v[3], v[2], v[1]), ...quad(v[4], v[5], v[6], v[7]), ...quad(v[0], v[1], v[5], v[4]),
    ...quad(v[2], v[3], v[7], v[6]), ...quad(v[1], v[2], v[6], v[5]), ...quad(v[0], v[4], v[7], v[3])]);
}

const topoOf = (pos) => buildTopology({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });

function fnv64(words) {
  let h = 0xcbf29ce484222325n;
  const bytes = new Uint8Array(words.buffer, words.byteOffset, words.byteLength);   // little-endian on this machine
  for (const b of bytes) { h ^= BigInt(b); h = (h * 0x100000001b3n) & 0xffffffffffffffffn; }
  return h.toString(16).padStart(16, "0");
}

const n0 = (v) => v ?? 0;
function summary(b) {
  const flat = (tris) => Float32Array.from(tris.flatMap((v) => [v[0], v[1], v[2]]));
  const fin = flat(b.triangles), pad = flat(b.padTriangles);
  const both = new Float32Array(fin.length + pad.length);
  both.set(fin); both.set(pad, fin.length);
  return {
    finVerts: b.triangles.length, padVerts: b.padTriangles.length, hash: fnv64(both),
    tines: b.tines, braceCount: n0(b.braceCount), propCount: n0(b.propCount), unserved: b.unserved, patchCount: b.patchCount,
    fins: b.fins.map((f) => ({
      id: f.id, kind: f.kind, height: n0(f.height), length: n0(f.length), tines: n0(f.tines), rows: n0(f.rows),
      stilt: n0(f.stilt), lean: n0(f.lean), bearing: n0(f.bearing), span: n0(f.span),
      short: !!f.short, fill: !!f.fill, joined: !!f.joined, braces: f.braces ?? null, ribs: f.ribs ?? null,
      triRanges: f.triRanges, line: (f.line ?? []).map((p) => [p[0], p[1], p[2]]),
    })),
    props: b.props.map((q) => ({
      id: q.id ?? null, region: q.region ?? null, tines: n0(q.tines), span: n0(q.span), height: n0(q.height),
      fill: !!q.fill, squat: !!q.squat, rimWall: !!q.rimWall, inBore: q.inBore ?? null,
    })),
  };
}

const BASE_DIALOG = { tineDensity: 1, layerHeight: 0.2, coverage: 0.5 };
const CASES = [
  { name: "cube X35, the page's defaults", part: { model: "cube" }, rot: rotX(35),
    mode: "auto", tunables: {}, dialog: {} },
  { name: "sphere X30, custom pad, 4 wall lines, rounded feet, PETG-ish clearances", part: { model: "sphere" }, rot: rotX(30),
    mode: "auto", dialog: {},
    tunables: { padStyle: "custom", padCustom: { h: 0.6, gap: 0.1, grip: 0.1, margin: 3 }, wallLines: 4, roundFeet: true,
                padH: 0.35, padGrab: -0.1, propGap: 0.3 } },
  { name: "lbracket X35, tall ribs off, height 40", part: { model: "lbracket" }, rot: rotX(35),
    mode: "auto", tallRibs: false, dialog: { tallRibs: false }, tunables: { tallRibsHeight: 40 } },
  { name: "sphere X30, Full coverage with curved walls", part: { model: "sphere" }, rot: rotX(30),
    mode: "full", curvedWalls: true, dialog: { mode: "full", curvedWalls: true, coverage: 1 }, tunables: { nozzle: 0.6 } },
  { name: "a 300 mm post, sway braces at full depth with cross ribs, tall ribs on", part: { block: [-20, 20, -15, 15, 0, 300] },
    rot: [1, 0, 0, 0, 1, 0, 0, 0, 1], mode: "auto", tallRibs: true,
    sway: { gripFrom: 0, tineSpacing: 6, reach: 0.3, uncappedDepth: true, crossRise: 0.3 },
    dialog: { "sway.on": true, "sway.gripFrom": 0, "sway.tineSpacing": 6, "sway.reach": 0.3, "sway.uncappedDepth": true, "sway.crossRise": 0.3, tallRibs: true },
    tunables: { tallRibsHeight: 100, wallLines: 4, roundFeet: true } },
  { name: "the same post, tall ribs off: no cross ribs", part: { block: [-20, 20, -15, 15, 0, 300] },
    rot: [1, 0, 0, 0, 1, 0, 0, 0, 1], mode: "auto", tallRibs: false,
    sway: { gripFrom: 0, tineSpacing: 6, reach: 0.3, uncappedDepth: false, crossRise: 0.5 },
    dialog: { "sway.on": true, "sway.gripFrom": 0, "sway.tineSpacing": 6, "sway.reach": 0.3, "sway.uncappedDepth": false, "sway.crossRise": 0.5, tallRibs: false },
    tunables: {} },
];

const out = [];
for (const c of CASES) {
  const pos = c.part.model ? readSTL(Deno.readFileSync(`${UPSTREAM_ROOT}/prototype/stress/models/${c.part.model}.stl`)) : block(...c.part.block);
  const topo = topoOf(pos);
  const res = analyze(topo, 45, c.rot);
  const tunables = { ...DEFAULTS, ...c.tunables };   // the page always sends the full set
  const tallRibs = c.tallRibs ?? true;
  const request = {
    mode: c.mode, bedPad: (c.dialog.padStyle ?? "auto") !== "off", plateOnly: false, longWalls: true, separateWalls: false,
    tallRibs, curvedWalls: c.curvedWalls ? true : undefined, tines: true, tineDensity: BASE_DIALOG.tineDensity,
    layerHeight: BASE_DIALOG.layerHeight, coverage: c.mode === "full" ? FIN.coverDefault : BASE_DIALOG.coverage,
    // swayOpts() (ui/finbuild.js)
    sway: c.sway ? { on: true, roundFeet: tunables.roundFeet, uncappedDepth: c.sway.uncappedDepth, gripFrom: c.sway.gripFrom,
      tineSpacing: c.sway.tineSpacing, reach: c.sway.reach, crossRise: c.sway.crossRise, gap: tunables.propGap, tines: true,
      iface: tunables.iface, layerHeight: 0.2, crossAbove: tallRibs ? tunables.tallRibsHeight : Infinity } : undefined,
    tunables,
  };
  const built = buildFins(topo, res, c.rot, request);
  out.push({ name: c.name, part: c.part, rot: c.rot, dialog: { ...BASE_DIALOG, mode: c.mode, ...c.dialog }, tunables, expected: summary(built) });
  console.log(`${c.name}: ${built.triangles.length} fin verts, ${built.fins.length} fins`);
}
Deno.writeTextFileSync(new URL("../corpus/ffibuild/cases.json", import.meta.url), JSON.stringify({ upstream: Deno.readTextFileSync(new URL("../../UPSTREAM", import.meta.url)).match(/commit=(\w+)/)[1], cases: out }));
