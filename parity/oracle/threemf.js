// Oracle recorder for web/threemf.js -> parity/corpus/threemf/*.jsonl.
//
// Direct cases (<fn>.jsonl), written below:
//   scanXML          args [xml]; out: the events, [["open", name, attrs, selfClosing] | ["close", name]]
//   meshXML          args [tris] (the optional triAttrs/kept arguments are functions: not recorded
//                    here, the writers below exercise them); out: the string
//   mergeColors      args [objs] where each obj is { colors, positions }; out PartColors | null
//   writeThreeMF     args [partTris, finTris, name, opts]; out Uint8Array (the Blob's bytes)
//   writePlate3MF    args [models, opts]; out Uint8Array
//   readThreeMF      args [Uint8Array]; out the result, or `throws`
//   readThreeMFParts args [Map name -> Uint8Array, opts]; out the result, or `throws`
//
// Captured cases (<fn>.captured.jsonl): the same functions as the upstream tests tests/threemf.test.js
// and tests/session.test.js really call them. The test files run unchanged against a copy of the
// tree in which threemf.js has its exports wrapped by rename (docs/PARITY.md, "Capture-by-rename")
// and zip.js's Blob carries its bytes (so a writer's output can be recorded synchronously).
// ids are '<test name>#<call index in that test>'.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { assertPinned, UPSTREAM_ROOT, WEB } from "./lib/pin.js";
import { encode, writeCases } from "./lib/encode.js";
import { upstreamStatus } from "./lib/capture.js";

assertPinned();
const statusBefore = upstreamStatus();

const T = await import(`${WEB}threemf.js`);
const { zipStore } = await import(`${WEB}zip.js`);
const { block } = await import(`${WEB}../tests/_util.js`);
const bytesOf = async (blob) => new Uint8Array(await blob.arrayBuffer());
const enc = (s) => new TextEncoder().encode(s);
const triples = (flat) => { const o = []; for (let i = 0; i < flat.length; i += 3) o.push([flat[i], flat[i + 1], flat[i + 2]]); return o; };

// =============================================================== direct cases
const direct = { scanXML: [], meshXML: [], mergeColors: [], writeThreeMF: [], writePlate3MF: [], readThreeMF: [], readThreeMFParts: [] };

// --- scanXML ------------------------------------------------------------------------------
const events = (xml) => {
  const out = [];
  T.scanXML(xml, (n, a, s) => out.push(["open", n, a, s]), (n) => out.push(["close", n]));
  return out;
};
const xmls = {
  empty: "", text: "no tags", simple: "<a x='1' y=\"2\"/>", nested: "<a><b c='d'>text</b><e/></a>",
  decl: '<?xml version="1.0" encoding="UTF-8"?>\n<model unit="inch"><x/></model>',
  comment: "<a><!-- <b/> --><c/></a>", comment_unterminated: "<a><!-- <b/> <c/>", comment_short: "<a><!--></a>",
  cdata: "<a><![CDATA[ <b/> ]]><c/></a>", cdata_unterminated: "<a><![CDATA[ <b/> <c/>", doctype: "<!DOCTYPE x [<!ENTITY e 'v'>]><a/>",
  pi_unterminated: "<a/><?pi never ends",
  gt_in_value: '<object name="a > b" id="1"/>', single_quotes: "<a b='say \"hi\"'/>", unquoted: "<a b=1 c=2/>", unquoted_slash: "<a b=1/>",
  no_value: "<a b c='1' d/>", spaces_around_eq: "<a b = '1' c\t=\n\"2\"/>", empty_value: '<a b="" c=\'\'/>',
  unterminated_quote: '<a b="never closed', unterminated_tag: "<a b='1'", only_lt: "<", lt_end: "<a", close_only: "</a>", close_spaces: "</ a >",
  close_unterminated: "</a", ns_prefix: '<m:object p:path="/3D/x.model" id="3"/>', dup_attr: '<a b="1" b="2"/>',
  equals_first: "<a =x>", trailing_text: "<a/>tail", entities: '<a b="&lt;&amp;&quot;"/>',
  unicode: '<a n="café 中" é="1"/>', nbsp_ws: "<a b='1'　c='2'/>", crlf: "<a\r\nb='1'\r\n/>", newline_in_tag: "<a\n  b='1'\n  c='2'\n>",
  empty_name: "< />", numeric_like: "<a b='1'/><a b='2'/>", deep: "<a><b><c><d/></c></b></a>", angle_in_cdata_end: "<![CDATA[x]]>",
  closing_in_attrs: '<a b="</a>"/>', question_in_name: "<a?b/>",
};
for (const [id, xml] of Object.entries(xmls)) direct.scanXML.push({ id, args: [xml], out: events(xml) });

// --- meshXML ------------------------------------------------------------------------------
const meshes = {
  empty: [], one: [[0, 0, 0], [1, 0, 0], [0, 1, 0]], cube: triples(block(0, 1, 0, 1, 0, 1)),
  degenerate: [[0, 0, 0], [0, 0, 0], [1, 0, 0], [0, 0, 0], [1, 1, 1], [1, 1, 1]], negzero_merge: [[0, 0, 0], [-0, 1, 0], [1, 1, -0], [0, 0, 0], [-0, 0, -0], [1, 1, 1]],
  nan: [[NaN, 0, 0], [1, 0, 0], [0, 1, 0]], inf: [[Infinity, 0, 0], [-Infinity, 0, 0], [0, 1, 0]],
  fmt: [[1e21, 1e-7, -0.0000004], [123456789.123456789, 0.1 + 0.2, 1 / 3], [-2.5, 100, 0.0000005], [0.9999995, 0.0000015, -0.0000015], [5, 5, 5], [6, 6, 6]],
  tilted: triples(block(-5, 5, -5, 5, 0, 3)).map(([x, y, z]) => [x, y * Math.cos(0.7) - z * Math.sin(0.7), y * Math.sin(0.7) + z * Math.cos(0.7)]),
};
for (const [id, t] of Object.entries(meshes)) direct.meshXML.push({ id, args: [t], out: T.meshXML(t) });

// --- mergeColors --------------------------------------------------------------------------
const sparse = (len, entries) => { const a = new Array(len); for (const [i, v] of entries) a[i] = v; return a; };
const P = (name, v) => ({ name, v });
const merges = {
  none: [{ positions: new Float32Array(18) }, { positions: new Float32Array(9) }],
  one: [{ colors: { paint: [P("paint_color", "8")], rgb: ["#FF0000"] }, positions: new Float32Array(9) }],
  two: [{ colors: { paint: sparse(2, [[1, P("paint_color", "4")]]), rgb: sparse(2, [[0, "#00FF00"]]) }, positions: new Float32Array(18) },
    { positions: new Float32Array(27) }, { colors: { paint: [P("x", "8"), , P("y", "C")], rgb: [, "#0000FF"] }, positions: new Float32Array(9) }],
  empty: [], emptyColors: [{ colors: { paint: [], rgb: [] }, positions: new Float32Array(9) }],
  fractional: [{ colors: { paint: [P("a", "4")], rgb: [] }, positions: new Float32Array(12) }, { colors: { paint: [P("b", "8")], rgb: [] }, positions: new Float32Array(9) }],
};
for (const [id, o] of Object.entries(merges)) direct.mergeColors.push({ id, args: [o], out: T.mergeColors(o) });

// --- writers ------------------------------------------------------------------------------
const CUBE = triples(block(0, 1, 0, 1, 0, 1));
const BAR = triples(block(-20, 20, -3, 3, 0, 5));
const FINS = triples(block(-1, 1, -1, 1, 5, 35));
const CREST = triples(block(-1.5, 1.5, -1.5, 1.5, 35, 36));
const tilt = (tris, a) => tris.map(([x, y, z]) => [x, y * Math.cos(a) - z * Math.sin(a), y * Math.sin(a) + z * Math.cos(a)]);
const SLAB = tilt(triples(block(-20, 20, -15, 15, 0, 30)), 0.6154797086703874);
const colors1 = { paint: sparse(12, [[0, P("paint_color", "8")], [3, P("paint_color", "0C")], [11, P("paint_color", "4")]]), rgb: sparse(12, [[1, "#FF0000"], [2, "#00FF00"], [5, "#FF0000"], [11, "#0000FF80"]]) };
const colorsPE = { paint: sparse(12, [[2, P("slic3rpe:mmu_segmentation", "8")], [7, P("slic3rpe:mmu_segmentation", "801")]]), rgb: [] };
const settings = { bambu: [["extruder", "2"], ["wall_loops", "3"], ["sparse_infill_density", "15%"]], prusa3: { extruder: 2, 5: "five", infill: { density: "20%", list: [1, 2.5, "x"] }, wall: 3, "<k>": '"q"\n' }, prusa2: [["infill_every_layers", "2"], ["<k>", 'a"b&c']] };
const ex = (kind, name, tris, s) => ({ kind, name, settings: s, tris });
const NEG = triples(block(-2, 2, -1, 1, 1, 3)), MOD = triples(block(10, 14, -2, 2, 0, 4)), BLK = triples(block(-12, -8, -2, 2, 0, 4)), ENF = triples(block(16, 18, -2, 2, 0, 4));
const wcase = (id, ...a) => direct.writeThreeMF.push({ id, args: a, out: null });
const W = (id, partTris, finTris, name, opts) => direct.writeThreeMF.push({ id, args: opts === undefined ? [partTris, finTris, name] : [partTris, finTris, name, opts], out: bytesOf(T.writeThreeMF(...(opts === undefined ? [partTris, finTris, name] : [partTris, finTris, name, opts]))) });
void wcase;
W("locked", BAR, FINS, "bar");
W("locked-default-name", CUBE, []);
W("locked-no-fins", CUBE, [], "cube");
W("separate", BAR, FINS, "bar", { separate: true });
W("separate-no-fins", CUBE, [], "cube", { separate: true });
W("separate-wide", triples(block(-120, 120, -30, 30, 0, 5)), triples(block(-1, 1, -1, 1, 5, 35)), "wide", { separate: true });
W("separate-iface", BAR, FINS, "bar", { separate: true, iface: CREST });
W("locked-iface", BAR, FINS, "bar", { iface: CREST });
W("locked-iface-no-fins", BAR, [], "bar", { iface: CREST });
W("session", BAR, FINS, "shelf", { separate: true, session: { v: 1, part: { size: [80, 20, 39] }, finMode: "draw", finsVisible: true, form: { gap: "0.25" }, walls: [{ a: [1, 2, 3], b: [4, 5, 6] }, { kind: "sway", a: [7, 8, 9] }], n: -0, big: 1e21, small: 1e-7, nan: NaN, s: "tab\t\"q\" \u0001é" } });
W("session-null", BAR, FINS, "x", { session: null });
W("name-special", CUBE, FINS, '<a & "b">', { separate: true });
W("name-unicode", CUBE, FINS, "café 中文 \u{1F600}");
W("slab", SLAB, FINS, "slab", { separate: true });
W("colors", CUBE.concat(tilt(CUBE, 0.3)).slice(0, 12 * 3), [], "c", { colors: colors1 });
W("colors-separate", BAR, FINS, "c", { separate: true, colors: colors1 });
W("colors-paint-only", BAR, [], "c", { colors: { paint: sparse(12, [[4, P("paint_color", "8")]]), rgb: [] } });
W("colors-rgb-only", BAR, [], "c", { colors: { paint: [], rgb: sparse(12, [[4, "#123456"], [5, "#123456"], [6, "#ABCDEF"]]) } });
W("colors-empty", BAR, [], "c", { colors: { paint: [], rgb: [] } });
W("colors-prusa", BAR, [], "c", { colors: colorsPE });
W("colors-escape", BAR, [], "c", { colors: { paint: sparse(12, [[1, P("paint_color", 'a"<&>')]]), rgb: sparse(12, [[2, '#"<x']]) } });
W("colors-degenerate", [[0, 0, 0], [0, 0, 0], [1, 0, 0], [0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1], [1, 0, 1], [0, 1, 1]], [], "c", { colors: { paint: sparse(3, [[0, P("paint_color", "4")], [1, P("paint_color", "8")], [2, P("paint_color", "0C")]]), rgb: sparse(3, [[0, "#111111"], [2, "#222222"]]) } });
W("colors-iface", BAR, FINS, "c", { separate: true, iface: CREST, colors: colors1 });
W("extras-bambu", BAR, FINS, "bar", { separate: true, extras: [ex("negative", "hole", NEG, { bambu: [["wall_loops", "4"]] }), ex("modifier", "", MOD, settings), ex("blocker", "blk", BLK, {}), ex("enforcer", "enf", ENF, { prusa3: { a: 1 } })] });
W("extras-bambu-locked", BAR, FINS, "bar", { extras: [ex("negative", "hole", NEG, {})] });
W("extras-bambu-no-fins", BAR, [], "bar", { separate: true, extras: [ex("negative", "hole", NEG, { bambu: [] })] });
W("extras-prusa2", BAR, FINS, "bar", { separate: true, extras: [ex("negative", "hole", NEG, { prusa2: [] }), ex("modifier", "mod", MOD, { prusa2: settings.prusa2 })] });
W("extras-prusa2-colors", BAR, FINS, "bar", { separate: true, colors: colorsPE, extras: [ex("modifier", "mod", MOD, { prusa2: [["a", "1"]] })] });
W("extras-prusa2-degenerate", BAR, [], "bar", { separate: true, extras: [ex("negative", "n", [[0, 0, 0], [0, 0, 0], [1, 1, 1], ...NEG], { prusa2: [] }), ex("modifier", "", [[5, 5, 5], [5, 5, 5], [5, 5, 5]], { prusa2: [] })] });
W("extras-mixed-settings", BAR, FINS, "bar", { separate: true, extras: [ex("negative", "a", NEG, { prusa2: [["x", "1"]], bambu: [["y", "2"]] })] });
W("extras-iface-colors", BAR, FINS, "bar", { separate: true, iface: CREST, colors: colors1, extras: [ex("modifier", "m", MOD, { bambu: [["k", "v"]] })] });
W("extras-empty-tris", BAR, FINS, "bar", { separate: true, extras: [ex("negative", "n", [], {})] });

const wp = (id, models, opts) => direct.writePlate3MF.push({ id, args: opts === undefined ? [models] : [models, opts], out: bytesOf(T.writePlate3MF(...(opts === undefined ? [models] : [models, opts]))) });
const m = (name, partTris, finTris, rest = {}) => ({ partTris, finTris, name, ...rest });
const shift = (tris, dx) => tris.map(([x, y, z]) => [x + dx, y, z]);
wp("one", [m("a", BAR, FINS)]);
wp("one-title", [m("a", BAR, FINS)], { title: "Plate" });
wp("two", [m("a", BAR, FINS), m("b", shift(BAR, 60), shift(FINS, 60))]);
wp("two-separate", [m("a", BAR, FINS), m("b", shift(BAR, 60), shift(FINS, 60))], { separate: true });
wp("two-bed", [m("a", BAR, FINS), m("b", shift(BAR, 60), [])], { separate: true, bed: { x: 220, y: 220 } });
wp("lone-bed", [m("a", CUBE, [])], { bed: { x: 180, y: 200.5 } });
wp("lone-bed-separate", [m("a", CUBE, FINS)], { separate: true, bed: { x: 256, y: 256 } });
wp("two-session", [m("a", BAR, FINS), m("b", shift(BAR, 60), [])], { separate: true, session: { v: 1, models: [1, 2] } });
wp("colors-iface-extras", [m("a", BAR, FINS, { iface: CREST, colors: colors1, extras: [ex("negative", "n", NEG, { bambu: [], prusa3: {} })] }),
  m("b", shift(BAR, 60), shift(FINS, 60), { iface: shift(CREST, 60), colors: colors1, extras: [ex("modifier", "m", shift(MOD, 60), { bambu: [["k", "v"]] })] })], { separate: true });
wp("prusa2-two", [m("a", BAR, FINS, { extras: [ex("negative", "n", NEG, { prusa2: [] })] }), m("b", shift(BAR, 60), FINS, { extras: [ex("modifier", "m", MOD, { prusa2: [["q", "1"]] })] })], { separate: true });
wp("prusa2-then-bambu", [m("a", BAR, FINS, { extras: [ex("negative", "n", NEG, { prusa2: [] })] }), m("b", shift(BAR, 60), FINS, { extras: [ex("negative", "n2", shift(NEG, 60), { bambu: [] })] })], { separate: true });
wp("paint-on-second", [m("a", BAR, FINS), m("b", shift(BAR, 60), FINS, { colors: colorsPE })], { separate: true });
wp("locked-two", [m("a", BAR, FINS), m("b", shift(BAR, 60), FINS, { iface: CREST })]);
wp("fin-empty-undefined", [{ partTris: BAR, name: "u" }, { partTris: shift(BAR, 30), finTris: [], name: "v" }], { separate: true });
wp("no-names", [{ partTris: BAR, finTris: FINS }]);
wp("empty-models", []);

// --- readers ------------------------------------------------------------------------------
const reads = new Map();
const R = (id, bytes) => reads.set(id, bytes);
for (const f of ["tests/fixtures/negative-parts/bambu.3mf", "tests/fixtures/negative-parts/orca.3mf", "tests/fixtures/negative-parts/prusaslicer3.3mf",
  "prototype/calibration/tine/print/tine-coupon.3mf", "prototype/calibration/grip/print/grip-coupon-split.3mf",
  "prototype/calibration/foot/print/foot-coupon.3mf", "prototype/calibration/lip/print/lip-coupon.3mf"]) R(`file:${f}`, new Uint8Array(readFileSync(join(UPSTREAM_ROOT, f))));
// everything the writer cases above wrote
for (const c of direct.writeThreeMF) R(`written:${c.id}`, await c.out);
for (const c of direct.writePlate3MF) R(`plate:${c.id}`, await c.out);
for (const [id, bytes] of reads) {
  try { direct.readThreeMF.push({ id, args: [bytes], out: await T.readThreeMF(bytes) }); } catch (e) { direct.readThreeMF.push({ id, args: [bytes], throws: e.message }); }
}

// hand-made packages
const CORE = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
const REL = "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel";
const tri = (e = 1, z = 0) => `<mesh><vertices><vertex x="0" y="0" z="${z}"/><vertex x="${e}" y="0" z="${z}"/><vertex x="0" y="${e}" z="${z}"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh>`;
const model = (resources, build = "", attrs = `unit="millimeter"`) => `<?xml version="1.0"?><model ${attrs} xmlns="${CORE}" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06"><resources>${resources}</resources>${build ? `<build>${build}</build>` : ""}</model>`;
const pack = async (parts, rels = true) => {
  const list = [{ name: "[Content_Types].xml", data: '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>' }];
  if (rels) list.push({ name: "_rels/.rels", data: `<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="r" Target="/3D/3dmodel.model" Type="${REL}"/></Relationships>` });
  for (const [name, data] of Object.entries(parts)) list.push({ name, data });
  return bytesOf(zipStore(list));
};
const M = (xml) => ({ "3D/3dmodel.model": xml });
const O = (id, inner, attrs = "") => `<object id="${id}" type="model"${attrs}>${inner}</object>`;
const hand = {
  millimeter: M(model(O(1, tri()), '<item objectid="1"/>')),
  inch: M(model(O(1, tri()), '<item objectid="1"/>', 'unit="inch"')),
  micron: M(model(O(1, tri(1000)), '<item objectid="1"/>', 'unit="micron"')),
  foot_meter_cm: M(model(O(1, tri()) + O(2, tri()) + O(3, tri()), '<item objectid="1"/><item objectid="2"/><item objectid="3"/>', 'unit="foot"')),
  unit_unknown: M(model(O(1, tri()), '<item objectid="1"/>', 'unit="furlong"')),
  unit_empty: M(model(O(1, tri()), '<item objectid="1"/>', 'unit=""')),
  item_transform: M(model(O(1, tri()), '<item objectid="1" transform="1 0 0 0 1 0 0 0 1 10 20 30"/>')),
  item_transform_rot: M(model(O(1, tri()), '<item objectid="1" transform="0 1 0 -1 0 0 0 0 1 5 0 0"/>')),
  item_transform_bad: M(model(O(1, tri()), '<item objectid="1" transform="1 0 0 0 1 0 0 0 1 10 20"/>')),
  item_transform_nan: M(model(O(1, tri()), '<item objectid="1" transform="1 0 0 0 1 0 0 0 1 10 20 x"/>')),
  item_transform_hex: M(model(O(1, tri()), '<item objectid="1" transform="0x1 0 0 0 1 0 0 0 1 0 0 0"/>')),
  item_transform_empty: M(model(O(1, tri()), '<item objectid="1" transform=""/>')),
  compose: M(model(O(1, tri()) + O(2, '<components><component objectid="1" transform="1 0 0 0 1 0 0 0 1 1 0 0"/></components>'), '<item objectid="2" transform="2 0 0 0 2 0 0 0 2 0 5 0"/>')),
  mirrored: M(model(O(1, tri()), '<item objectid="1" transform="-1 0 0 0 1 0 0 0 1 0 0 0"/>')),
  no_build: M(model(O(1, tri()) + O(2, tri(2)))),
  no_build_component: M(model(O(1, tri()) + O(2, '<components><component objectid="1"/></components>'))),
  empty_build: M(model(O(1, tri()), "")),
  skip_types: M(model(O(1, tri(), "").replace('type="model"', 'type="support"') + O(2, tri(2)) + `<object id="3" type="surface">${tri(3)}</object><object id="4" type="other">${tri(4)}</object><object id="5" type="solidsupport">${tri(5)}</object>`, '<item objectid="1"/><item objectid="2"/><item objectid="3"/><item objectid="4"/><item objectid="5"/>')),
  all_skipped: M(model(`<object id="1" type="support">${tri()}</object>`, '<item objectid="1"/>')),
  missing_vertex: M(model(O(1, '<mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/><triangle v1="0" v2="1" v3="9"/><triangle v1="-1" v2="1" v3="2"/><triangle v1="0" v2="1" v3="x"/><triangle v1="0" v2="1" v3="1.5"/><triangle v1="0" v2="1"/><triangle v1="0" v2="1" v3="2"/></triangles></mesh>'), '<item objectid="1"/>')),
  no_geometry: M(model(O(1, ""), '<item objectid="1"/>')),
  dangling_item: M(model(O(1, tri()), '<item objectid="1"/><item objectid="99"/>')),
  dangling_component: M(model(O(1, tri()) + O(2, '<components><component objectid="1"/><component objectid="77"/></components>'), '<item objectid="2"/>')),
  cycle: M(model(O(1, tri() + '<components><component objectid="1"/></components>'), '<item objectid="1"/>')),
  cycle2: M(model(O(1, '<components><component objectid="2"/></components>') + O(2, tri() + '<components><component objectid="1"/></components>'), '<item objectid="1"/>')),
  object_self_closing: M(model('<object id="1" type="model"/>' + O(2, tri()), '<item objectid="2"/>')),
  nonvertex_vertices: M(model(O(1, '<mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0"/><vertex x="a" y=" 2 " z="1e1"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh>'), '<item objectid="1"/>')),
  vertices_self_closing: M(model(O(1, '<mesh><vertices/><vertex x="5" y="5" z="5"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh>'), '<item objectid="1"/>')),
  prefixed: M(model(O(1, tri()), '<item objectid="1"/>').replaceAll("<object", "<m:object").replaceAll("</object", "</m:object").replaceAll("<item", "<m:item")),
  production_path: { "3D/3dmodel.model": model(O(10, '<components><component p:path="/3D/Objects/a.model" objectid="1"/></components>'), '<item objectid="10"/>'), "3D/Objects/a.model": model(O(1, tri(3))) },
  production_relative: { "3D/3dmodel.model": model(O(10, '<components><component p:path="3D/Objects/a.model" objectid="1"/></components>'), '<item objectid="10"/>'), "3D/Objects/a.model": model(O(1, tri(3))) },
  production_prefix_other: { "3D/3dmodel.model": model(O(10, '<components><component q:path="/3D/Objects/a.model" objectid="1" xmlns:q="x"/></components>'), '<item objectid="10"/>'), "3D/Objects/a.model": model(O(1, tri(3))) },
  production_chain: { "3D/3dmodel.model": model(O(10, '<components><component p:path="/3D/a.model" objectid="1"/></components>'), '<item objectid="10"/>'), "3D/a.model": model(O(1, '<components><component p:path="/3D/b.model" objectid="1"/></components>')), "3D/b.model": model(O(1, tri(4))) },
  production_collide: { "3D/3dmodel.model": model(O(10, '<components><component p:path="/3D/Objects/a.model" objectid="1"/></components>') + O(20, '<components><component p:path="/3D/Objects/b.model" objectid="1"/></components>'), '<item objectid="10"/><item objectid="20"/>'), "3D/Objects/a.model": model(O(1, tri(1))), "3D/Objects/b.model": model(O(1, tri(5))) },
  production_missing_part: { "3D/3dmodel.model": model(O(10, '<components><component p:path="/3D/Objects/none.model" objectid="1"/></components>'), '<item objectid="10"/>') },
  production_unit: { "3D/3dmodel.model": model(O(10, '<components><component p:path="/3D/a.model" objectid="1"/></components>'), '<item objectid="10"/>', 'unit="inch"'), "3D/a.model": model(O(1, tri(2)), "", 'unit="millimeter"') },
  basematerials: M(model('<basematerials id="5"><base name="a" displaycolor="#FF0000"/><base name="b" displaycolor="#00FF00"/></basematerials>' + O(1, tri().replace('<triangle v1="0" v2="1" v3="2"/>', '<triangle v1="0" v2="1" v3="2"/><triangle v1="0" v2="1" v3="2" p1="1"/><triangle v1="0" v2="1" v3="2" p1="7"/><triangle v1="0" v2="1" v3="2" pid="9"/>'), ' pid="5" pindex="0"'), '<item objectid="1"/>')),
  colorgroup: M(model('<m:colorgroup id="7"><m:color color="#AA0000"/><m:color color="#00BB00"/><m:color/></m:colorgroup>' + O(1, tri().replace('<triangle v1="0" v2="1" v3="2"/>', '<triangle v1="0" v2="1" v3="2" pid="7" p1="1"/><triangle v1="0" v2="1" v3="2" pid="7" p1="2"/><triangle v1="0" v2="1" v3="2" pid="7" p1="-0"/><triangle v1="0" v2="1" v3="2" pid="7" p1="0.5"/><triangle v1="0" v2="1" v3="2" pid="7" p1="1e0"/>')), '<item objectid="1"/>')),
  group_self_closing: M(model('<basematerials id="5"/><base displaycolor="#FF0000"/>' + O(1, tri().replace('<triangle v1="0" v2="1" v3="2"/>', '<triangle v1="0" v2="1" v3="2" pid="5"/>')), '<item objectid="1"/>')),
  paint: M(model(O(1, tri().replace('<triangle v1="0" v2="1" v3="2"/>', '<triangle v1="0" v2="1" v3="2" paint_color="8"/><triangle v1="0" v2="1" v3="2" slic3rpe:mmu_segmentation="8"/><triangle v1="0" v2="1" v3="2" paint_color="" slic3rpe:mmu_segmentation="0C"/><triangle v1="0" v2="1" v3="2" paint_color="4" slic3rpe:mmu_segmentation="4"/>')), '<item objectid="1"/>')),
  paint_default_pid: M(model('<basematerials id="1"><base displaycolor="#123456"/></basematerials>' + O(1, tri(), ' pid="1" pindex="0"'), '<item objectid="1"/>')),
  object_names: M(model(O(1, tri(), ' name="Part One"') + O(2, tri(), ' name=""'), '<item objectid="1"/><item objectid="2"/>')),
  session_ok: { ...M(model(O(1, tri()), '<item objectid="1"/>')), "Metadata/support_fins.json": '{"v":1,"walls":[{"a":[1,2,3]}],"n":-0,"1":"one","a":2,"0":"zero"}' },
  session_v2: { ...M(model(O(1, tri()), '<item objectid="1"/>')), "Metadata/support_fins.json": '{"v":2}' },
  session_bad: { ...M(model(O(1, tri()), '<item objectid="1"/>')), "Metadata/support_fins.json": "{not json" },
  session_array: { ...M(model(O(1, tri()), '<item objectid="1"/>')), "Metadata/support_fins.json": "[1]" },
  session_string_v: { ...M(model(O(1, tri()), '<item objectid="1"/>')), "Metadata/support_fins.json": '{"v":"1"}' },
  session_dup_keys: { ...M(model(O(1, tri()), '<item objectid="1"/>')), "Metadata/support_fins.json": '{"v":0,"a":1,"v":1}' },
  session_escapes: { ...M(model(O(1, tri()), '<item objectid="1"/>')), "Metadata/support_fins.json": '{"v":1,"s":"\\u00e9\\n\\ud83d\\ude00\\/","e":1E2,"f":-1.5e-3}' },
};
for (const [id, parts] of Object.entries(hand)) R(`hand:${id}`, await pack(parts));
R("hand:no_rels_root_by_name", await pack(M(model(O(1, tri()), '<item objectid="1"/>')), false));
R("hand:root_by_rels", await pack({ "x/model1.model": model(O(1, tri(2)), '<item objectid="1"/>') }, false).then(async () => {
  return bytesOf(zipStore([{ name: "_rels/.rels", data: `<Relationships><Relationship Id="r" Target="/x/model1.model" Type="${REL}"/></Relationships>` }, { name: "x/model1.model", data: model(O(1, tri(2)), '<item objectid="1"/>') }]));
}));
R("hand:root_by_rels_missing", await bytesOf(zipStore([{ name: "_rels/.rels", data: `<Relationships><Relationship Id="r" Target="/gone.model" Type="${REL}"/></Relationships>` }, { name: "a/o.model", data: model(O(1, tri(2))) }, { name: "b/p.model", data: model(O(1, tri(3)), '<item objectid="1"/>') }])));
R("hand:root_by_build_tag", await bytesOf(zipStore([{ name: "a/o.model", data: model(O(1, tri(2))) }, { name: "b/p.MODEL", data: model(O(1, tri(3)), '<item objectid="1"/>') }])));
R("hand:root_first_model", await bytesOf(zipStore([{ name: "a/o.model", data: model(O(1, tri(2))) }, { name: "b/p.model", data: model(O(1, tri(3))) }])));
R("hand:no_model", await bytesOf(zipStore([{ name: "a.txt", data: "x" }])));
R("hand:not_zip", enc("PK nothing"));
R("hand:empty_zip", await bytesOf(zipStore([])));
// slicer configs
const bambuCfg = (inner) => `<?xml version="1.0"?><config>${inner}</config>`;
const asm = (parts) => model(O(1, tri()) + O(2, tri(2)) + O(3, tri(3)) + O(4, tri(4)) + O(10, `<components>${parts.map((p) => `<component objectid="${p}"/>`).join("")}</components>`), '<item objectid="10"/>');
const BAMBU = bambuCfg('<object id="10"><metadata key="name" value="Bracket.stl"/><metadata key="extruder" value="2"/><part id="1" subtype="normal_part"><metadata key="name" value="main.stl"/></part><part id="2" subtype="negative_part"><metadata key="name" value="cut.stl"/><metadata key="matrix" value="1 0 0"/><metadata key="wall_loops" value="5"/><metadata key="extruder" value="3"/></part><part id="3" subtype="modifier_part"><metadata key="source_file" value="x"/><metadata key="infill" value="40%"/></part><part id="4" subtype="support_blocker"/></object>');
const cfgs = {
  bambu_parts: { ...M(asm([1, 2, 3, 4])), "Metadata/model_settings.config": BAMBU },
  bambu_names_only: { ...M(asm([1])), "Metadata/model_settings.config": bambuCfg('<object id="10"><metadata key="name" value="Thing.STEP"/><part id="1"><metadata key="name" value="p.3mf"/></part></object>') },
  bambu_type_other: { ...M(model(O(1, tri()) + `<object id="2" type="other">${tri(2)}</object>` + O(10, '<components><component objectid="1"/><component objectid="2"/></components>'), '<item objectid="10"/>')), "Metadata/model_settings.config": bambuCfg('<object id="10"><part id="2" subtype="negative_part"/><part id="1" subtype="normal_part"/></object>') },
  bambu_part_twice: { ...M(model(O(1, tri()) + O(2, tri(2)) + O(10, '<components><component objectid="1"/><component objectid="2"/><component objectid="2" transform="1 0 0 0 1 0 0 0 1 9 0 0"/></components>'), '<item objectid="10"/>')), "Metadata/model_settings.config": bambuCfg('<object id="10"><part id="2" subtype="negative_part"><metadata key="name" value="n"/></part></object>') },
  bambu_self_closing_part: { ...M(asm([1, 2])), "Metadata/model_settings.config": bambuCfg('<object id="10"><part id="2" subtype="modifier_part"/><part id="1"/></object>') },
  bambu_unknown_subtype: { ...M(asm([1, 2])), "Metadata/model_settings.config": bambuCfg('<object id="10"><part id="2" subtype="whatever"><metadata key="name" value="w"/></part></object>') },
  bambu_empty_object_id: { ...M(asm([1, 2])), "Metadata/model_settings.config": bambuCfg('<object id=""><part id="2" subtype="negative_part"/></object><object><part id="2" subtype="negative_part"/></object>') },
  bambu_two_items: { ...M(model(O(1, tri()) + O(2, tri(2)) + O(10, '<components><component objectid="1"/></components>') + O(20, '<components><component objectid="1"/><component objectid="2"/></components>'), '<item objectid="10"/><item objectid="20"/>')), "Metadata/model_settings.config": bambuCfg('<object id="10"><metadata key="name" value="First"/></object><object id="20"><metadata key="name" value="Second"/><part id="2" subtype="negative_part"/></object>') },
  bambu_filament: { ...M(asm([1, 2])), "Metadata/model_settings.config": bambuCfg('<object id="10"><metadata key="extruder" value="2"/><part id="1" subtype="normal_part"><metadata key="extruder" value="3"/></part><part id="2" subtype="modifier_part"><metadata key="extruder" value="4"/></part></object>') },
  bambu_filament_nonnumeric: { ...M(asm([1])), "Metadata/model_settings.config": bambuCfg('<object id="10"><metadata key="extruder" value="x"/><part id="1"><metadata key="extruder" value="1.5"/></part></object>') },
  prusa3_json: { ...M(asm([1, 2, 3])), "Metadata/PrusaSlicer3_project.json": JSON.stringify({ objects: [{ id: 10, object_settings: { extruder: 2 }, volumes: [{ id: 1, type: "ModelPart", volume_settings: { extruder: 3 } }, { id: 2, type: "NegativeVolume", volume_settings: { infill: "10%", 7: "x", a: { b: [1, 2] } } }, { id: 3, type: "ParameterModifier" }] }] }) },
  prusa3_and_bambu: { ...M(asm([1, 2])), "Metadata/model_settings.config": bambuCfg('<object id="10"><part id="2" subtype="negative_part"><metadata key="k" value="v"/></part></object>'), "Metadata/PrusaSlicer3_project.json": '{"objects":[{"id":"10","volumes":[{"id":"2","type":"NegativeVolume","volume_settings":{"z":1}}]}]}' },
  prusa3_odd: { ...M(asm([1, 2])), "Metadata/PrusaSlicer3_project.json": '{"objects":[null,5,{"id":null},{"id":10,"volumes":[null,{"id":2,"type":["NegativeVolume"],"volume_settings":[1]}]}]}' },
  prusa3_bad_json: { ...M(asm([1, 2])), "Metadata/PrusaSlicer3_project.json": "{oops" },
  prusa3_not_array: { ...M(asm([1, 2])), "Metadata/PrusaSlicer3_project.json": '{"objects":{"id":10}}' },
  prusa3_extruder_forms: { ...M(asm([1, 2])), "Metadata/PrusaSlicer3_project.json": '{"objects":[{"id":10,"object_settings":{"extruder":"2"},"volumes":[{"id":1,"volume_settings":{"extruder":[3]}},{"id":2,"volume_settings":{"extruder":1e2}}]}]}' },
};
// PrusaSlicer 2: one mesh, volumes as triangle ranges
const quadMesh = '<mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/><vertex x="1" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/><triangle v1="1" v2="3" v3="2"/><triangle v1="0" v2="1" v3="3"/><triangle v1="0" v2="3" v3="2"/></triangles></mesh>';
const PE = (inner) => `<?xml version="1.0"?><config>${inner}</config>`;
const pe1 = M(model(O(1, quadMesh), '<item objectid="1"/>'));
Object.assign(cfgs, {
  prusa2_negative: { ...pe1, "Metadata/Slic3r_PE_model.config": PE('<object id="1" instances_count="1"><metadata type="object" key="name" value="Obj"/><volume firstid="0" lastid="1"><metadata type="volume" key="name" value="part"/></volume><volume firstid="2" lastid="3"><metadata type="volume" key="volume_type" value="NegativeVolume"/><metadata type="volume" key="name" value="neg.stl"/><metadata type="volume" key="matrix" value="1"/><metadata type="volume" key="infill" value="20%"/></volume></object>') },
  prusa2_modifier_legacy: { ...pe1, "Metadata/Slic3r_PE_model.config": PE('<object id="1"><volume firstid="3" lastid="3"><metadata type="volume" key="modifier" value="1"/></volume></object>') },
  prusa2_modifier_zero: { ...pe1, "Metadata/Slic3r_PE_model.config": PE('<object id="1"><volume firstid="3" lastid="3"><metadata key="modifier" value="0"/></volume></object>') },
  prusa2_all_kinds: { ...pe1, "Metadata/Slic3r_PE_model.config": PE('<object id="1"><volume firstid="0" lastid="0"><metadata key="volume_type" value="SupportBlocker"/></volume><volume firstid="1" lastid="1"><metadata key="volume_type" value="SupportEnforcer"/></volume><volume firstid="2" lastid="2"><metadata key="volume_type" value="ParameterModifier"/></volume></object>') },
  prusa2_bad_range: { ...pe1, "Metadata/Slic3r_PE_model.config": PE('<object id="1"><volume firstid="x" lastid="3"><metadata key="volume_type" value="NegativeVolume"/></volume><volume firstid="2"><metadata key="volume_type" value="NegativeVolume"/></volume></object>') },
  prusa2_unknown_type: { ...pe1, "Metadata/Slic3r_PE_model.config": PE('<object id="1"><volume firstid="0" lastid="3"><metadata key="volume_type" value="Weird"/></volume></object>') },
  prusa2_overlap: { ...pe1, "Metadata/Slic3r_PE_model.config": PE('<object id="1"><volume firstid="0" lastid="2"><metadata key="volume_type" value="NegativeVolume"/></volume><volume firstid="1" lastid="3"><metadata key="volume_type" value="ParameterModifier"/></volume></object>') },
  prusa2_instance: { ...M(model(O(1, quadMesh) + O(2, '<components><component objectid="1"/></components>') + O(3, '<components><component objectid="1" transform="1 0 0 0 1 0 0 0 1 5 0 0"/></components>'), '<item objectid="2"/><item objectid="3"/>')), "Metadata/Slic3r_PE_model.config": PE('<object id="1"><volume firstid="2" lastid="3"><metadata key="volume_type" value="NegativeVolume"/></volume></object>') },
  prusa2_filaments: { ...pe1, "Metadata/Slic3r_PE_model.config": PE('<object id="1"><metadata type="object" key="extruder" value="2"/><volume firstid="0" lastid="1"><metadata key="extruder" value="3"/></volume><volume firstid="2" lastid="3"><metadata key="extruder" value="0"/></volume></object>') },
  prusa2_filaments_painted: { ...M(model(O(1, quadMesh.replace('<triangle v1="0" v2="1" v3="2"/>', '<triangle v1="0" v2="1" v3="2" slic3rpe:mmu_segmentation="801"/>')), '<item objectid="1"/>')), "Metadata/Slic3r_PE_model.config": PE('<object id="1"><metadata key="extruder" value="2"/></object>') },
  prusa2_wrapper_filament: { ...M(model(O(1, quadMesh) + O(2, '<components><component objectid="1"/></components>'), '<item objectid="2"/>')), "Metadata/Slic3r_PE_model.config": PE('<object id="1"><metadata key="extruder" value="4"/></object>') },
});
for (const [id, parts] of Object.entries(cfgs)) R(`slicer:${id}`, await pack(parts));
for (const [id, bytes] of reads) {
  if (direct.readThreeMF.some((c) => c.id === id)) continue;
  try { direct.readThreeMF.push({ id, args: [bytes], out: await T.readThreeMF(bytes) }); } catch (e) { direct.readThreeMF.push({ id, args: [bytes], throws: e.message }); }
}

// readThreeMFParts with options, over the unzipped hand-made packages
const { unzip } = await import(`${WEB}zip.js`);
const optSets = { default: undefined, strict0: { buildIndex: 0 }, strict1: { buildIndex: 1 }, double: { doublePrecision: true }, outward: { outwardWinding: true },
  all: { buildIndex: 0, doublePrecision: true, outwardWinding: true } };
for (const [id, bytes] of reads) {
  if (!/^(hand:|slicer:)/.test(id) || /not_zip|empty_zip|no_model$/.test(id)) continue;
  const parts = await unzip(bytes);
  for (const [oid, opts] of Object.entries(optSets)) {
    if (oid === "default" && !/mirrored|compose|inch/.test(id)) continue;
    const args = opts === undefined ? [parts] : [parts, opts];
    try { direct.readThreeMFParts.push({ id: `${id}|${oid}`, args, out: T.readThreeMFParts(parts, opts) }); } catch (e) { direct.readThreeMFParts.push({ id: `${id}|${oid}`, args, throws: e.message }); }
  }
}
// strict-mode checks
const strictXml = {
  ok: model(O(1, tri()), '<item objectid="1"/>'),
  dup_id: model(O(1, tri()) + O(1, tri(2)), '<item objectid="1"/>'),
  bad_id_zero: model(O(0, tri()), '<item objectid="0"/>'),
  bad_id_leading_zero: model(O("01", tri()), '<item objectid="01"/>'),
  bad_id_text: model(O("a", tri()), '<item objectid="a"/>'),
  no_id: model(`<object type="model">${tri()}</object>`, ""),
  bad_vertex: model(O(1, tri().replace('x="1"', 'x="a"')), '<item objectid="1"/>'),
  missing_vertex_attr: model(O(1, tri().replace(' z="0"/><vertex x="1"', '/><vertex x="1"')), '<item objectid="1"/>'),
  blank_vertex_attr: model(O(1, tri().replace('x="1"', 'x=" "')), '<item objectid="1"/>'),
  bad_triangle: model(O(1, tri().replace('v3="2"', 'v3="x"')), '<item objectid="1"/>'),
  neg_triangle: model(O(1, tri().replace('v3="2"', 'v3="-1"')), '<item objectid="1"/>'),
  frac_triangle: model(O(1, tri().replace('v3="2"', 'v3="1.5"')), '<item objectid="1"/>'),
  unsafe_triangle: model(O(1, tri().replace('v3="2"', 'v3="9007199254740993"')), '<item objectid="1"/>'),
  missing_triangle_attr: model(O(1, tri().replace(' v3="2"', "")), '<item objectid="1"/>'),
  ref_past_end: model(O(1, tri().replace('v3="2"', 'v3="9"')), '<item objectid="1"/>'),
  singular_transform: model(O(1, tri()), '<item objectid="1" transform="1 0 0 0 1 0 0 0 0 0 0 0"/>'),
  bad_transform: model(O(1, tri()), '<item objectid="1" transform="1 2 3"/>'),
  empty_transform: model(O(1, tri()), '<item objectid="1" transform=""/>'),
  component_transform_bad: model(O(1, tri()) + O(2, '<components><component objectid="1" transform="0 0 0 0 0 0 0 0 0 0 0 0"/></components>'), '<item objectid="2"/>'),
  missing_component: model(O(1, '<components><component objectid="9"/></components>'), '<item objectid="1"/>'),
  cycle: model(O(1, tri() + '<components><component objectid="1"/></components>'), '<item objectid="1"/>'),
  second_item: model(O(1, tri()) + O(2, tri(2)), '<item objectid="1"/><item objectid="2"/>'),
};
for (const [id, xml] of Object.entries(strictXml)) {
  const parts = await unzip(await pack(M(xml)));
  for (const bi of [0, 1, 5]) {
    const args = [parts, { buildIndex: bi }];
    try { direct.readThreeMFParts.push({ id: `strict:${id}|${bi}`, args, out: T.readThreeMFParts(parts, { buildIndex: bi }) }); } catch (e) { direct.readThreeMFParts.push({ id: `strict:${id}|${bi}`, args, throws: e.message }); }
  }
}

for (const [fn, cases] of Object.entries(direct)) {
  const done = [];
  for (const c of cases) done.push({ ...c, ...("out" in c ? { out: await c.out } : {}) });
  writeCases("threemf", fn, done);
}

// =============================================================== captured cases
// The upstream tests, unchanged, against a wrapped copy of the tree.
function copyTree(from, to) {
  Deno.mkdirSync(to, { recursive: true });
  for (const e of Deno.readDirSync(from)) {
    const s = `${from}/${e.name}`, d = `${to}/${e.name}`;
    if (e.isDirectory) copyTree(s, d); else if (e.isFile) Deno.copyFileSync(s, d);
  }
}
const dir = Deno.makeTempDirSync({ prefix: "sf-capture-3mf-" });
copyTree(`${UPSTREAM_ROOT}/web`, `${dir}/web`);
copyTree(`${UPSTREAM_ROOT}/tests`, `${dir}/tests`);
copyTree(`${UPSTREAM_ROOT}/prototype/stress/models`, `${dir}/prototype/stress/models`);
// zip.js's Blob carries its bytes, so a writer's output is recorded synchronously
{
  const f = `${dir}/web/zip.js`;
  let src = Deno.readTextFileSync(f);
  const from = "return new Blob([buf], { type: 'application/vnd.ms-package.3dmanufacturing-3dmodel+xml' });";
  if (!src.includes(from)) throw new Error("capture: zip.js changed shape");
  Deno.writeTextFileSync(f, src.replace(from, `const __b = new Blob([buf], { type: 'application/vnd.ms-package.3dmanufacturing-3dmodel+xml' }); __b.__bytes = out; return __b;`));
}
const FNS = ["readThreeMF", "readThreeMFParts", "writeThreeMF", "writePlate3MF", "mergeColors"];
const sink = { tag: "", counts: {}, calls: Object.fromEntries(FNS.map((f) => [f, []])) };
const key = "__sfCapture3mf";
const settle = (entry, result) => { entry.out = encode(result instanceof Blob || result?.__bytes ? (result.__bytes ?? result) : result); };
globalThis[key] = {
  call(name, fn, self, args) {
    const n = sink.counts[name] = (sink.counts[name] ?? 0) + 1;
    const entry = { scene: sink.tag, n: n - 1, args: args.map(encode) };
    sink.calls[name].push(entry);
    let result;
    try { result = fn.apply(self, args); } catch (e) { entry.throws = String(e?.message ?? e); throw e; }
    if (result instanceof Promise) {
      return result.then((v) => { entry.out = encode(v); return v; }, (e) => { entry.throws = String(e?.message ?? e); throw e; });
    }
    if (result?.__bytes) entry.out = encode(new Uint8Array(result.__bytes));
    else entry.out = encode(result);
    void settle;
    return result;
  },
};
{
  const f = `${dir}/web/threemf.js`;
  let src = Deno.readTextFileSync(f);
  for (const fn of FNS) {
    const re = new RegExp(`^export (async )?function ${fn}\\(`, "gm");
    if ((src.match(re) ?? []).length !== 1) throw new Error(`capture: expected one 'export function ${fn}('`);
    src = src.replace(re, `$1function __${fn}(`);
  }
  src = `const __record = (name, fn) => function (...args) { return globalThis.${key}.call(name, fn, this, args); };\n` + src +
    FNS.map((fn) => `\nexport const ${fn} = __record('${fn}', __${fn});`).join("") + "\n";
  Deno.writeTextFileSync(f, src);
}
const tests = [];
const realTest = Deno.test;
Deno.test = (a, b) => { tests.push(typeof a === "object" ? [a.name, a.fn] : [a, b]); };
const root = new URL(`file://${dir}/`).href;
for (const file of ["tests/threemf.test.js", "tests/session.test.js"]) {
  tests.length = 0;
  await import(new URL(file, root).href);
  const mine = tests.slice();
  for (const [name, fn] of mine) {
    sink.tag = `${file.replace(/^tests\//, "").replace(/\.js$/, "")}:${name}`;
    sink.counts = {};
    try { await fn({}); } catch (e) { console.error(`captured run: ${sink.tag} failed: ${e.message}`); }
  }
}
Deno.test = realTest;
for (const [fn, calls] of Object.entries(sink.calls)) {
  const cases = calls.map((c) => ({ id: `${c.scene}#${c.n}`, args: c.args, ...(c.throws !== undefined ? { throws: c.throws } : { out: c.out }) }));
  writeCases("threemf", `${fn}.captured`, cases, { encoded: true });
}
Deno.removeSync(dir, { recursive: true });
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout was modified");
