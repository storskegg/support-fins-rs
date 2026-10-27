// Records parity/corpus/orient/*.jsonl (web/orient.js; direct cases).
//   layerVerdict         args: [{x, y, z}]                    out: {posture}
//   loadAlignment        args: [[dx, dy, dz]]                 out: {cross, quality, text} | null
//   suggestStrengthPose  args: [$topo, [dx, dy, dz], {threshold}]
//                        out: {rot, cross, quality, text, height, over, bedArea} | null
//   suggestOrientations  args: [$topo, {top, threshold}, {$config: <every config object>}]
//                        out: {candidates: [{rot, overArea, bedArea, height, size, regions, walls, coverage, volume, seating,
//                        holes, bore, score}], confidence}
// Replayed by crates/sf-testkit/tests/parity_orient.rs.
import { writeCases } from "./lib/encode.js";
import { modelList, topoRef, topoOf } from "./lib/direct.js";
import { WEB } from "./lib/pin.js";
import { encode, Ref } from "./lib/encode.js";

const { layerVerdict, loadAlignment, suggestStrengthPose, suggestOrientations } = await import(`${WEB}orient.js`);
const snapMods = {};
for (const [k, p] of Object.entries({
  prop: "prop/config.js", fin: "fins/config.js", pad: "fins/pad.js", perp: "fins/wedges.js", sway: "sway.js", cut: "cutout.js",
  island: "islands.js", trace: "prop/trace.js", tie: "prop/ties.js", brace: "prop/brace.js", resin: "prop/resinfoot.js",
  judge: "prop/judge.js", downhill: "prop/downhill.js", field: "fins/field.js", curved: "prop/curved.js", rim: "prop/rim.js",
  arc: "prop/arc.js", fill: "fins/fill.js", rows: "fins/rows.js", nozzle: "nozzle.js",
})) snapMods[k] = await import(WEB + p);
const m = snapMods;
const snap = () => ({
  PROP: m.prop.PROP, FIN: m.fin.FIN, PAD: m.pad.PAD, PERP: m.perp.PERP, SWAY: m.sway.SWAY, CUT: m.cut.CUT, ISLAND: m.island.ISLAND,
  TRACE: m.trace.TRACE, TIE: m.tie.TIE, BRACE: m.brace.BRACE, RESIN_FOOT: m.resin.RESIN_FOOT, JUDGE: m.judge.JUDGE,
  DOWNHILL: m.downhill.DOWNHILL, FIELD: m.field.FIELD, CURVED: m.curved.CURVED, RIM: m.rim.RIM, ARC: m.arc.ARC, FILL: m.fill.FILL,
  ROWS: m.rows.ROWS, NOZZLE: { nozzle: m.nozzle.nozzle, wallLines: m.nozzle.wallLines },
});

const cases = { layerVerdict: [], loadAlignment: [], suggestStrengthPose: [], suggestOrientations: [] };

const sizes = [];
for (const x of [0, 0.1, 5, 40, 100]) for (const y of [0, 5, 50, 100]) for (const z of [0, 0.5, 5, 50, 52.5, 70, 94.9, 95, 100]) sizes.push({ x, y, z });
for (const s of sizes) cases.layerVerdict.push({ id: `${s.x}x${s.y}x${s.z}`, args: [s], out: layerVerdict(s) });

const dirs = [[0, 0, 1], [0, 0, -1], [1, 0, 0], [0, 1, 0], [1, 0, 1], [Math.sqrt(3), 0, 1], [1, 1, 1], [0, 0, 0], [1e-10, 0, 0], [0.3, -0.2, 0.9], [-1, 2, -3], [1, 0, 1.7320508], [1, 0, 1.1547]];
for (const d of dirs) cases.loadAlignment.push({ id: d.join(","), args: [d], out: loadAlignment(d) });

for (const [name] of modelList()) {
  for (const d of [[0, 0, 1], [1, 0, 0], [0.3, -0.2, 0.9], [0, 0, 0], [-1, 2, -3]]) {
    for (const threshold of [45, 30]) {
      if (threshold === 30 && d[2] !== 1 && d[0] !== 1) continue;
      cases.suggestStrengthPose.push({ id: `${name}|${d.join(",")}|t${threshold}`, args: [topoRef(name), d, { threshold }],
        out: suggestStrengthPose(topoOf(name), d, { threshold }) });
    }
  }
}

// the minimise-support suggestions: every model, a few option sets. The module `topo` is reused across
// calls, so the finalists see the aliased flags exactly as the page does.
for (const [name] of modelList()) {
  for (const o of [{ top: 3, threshold: 45 }, { top: 1, threshold: 45 }, { top: 5, threshold: 30 }]) {
    if (o.top !== 3 && !/^(cube|wedge|lbracket|torus|arch|ramp)$/.test(name)) continue;
    const topo = topoOf(name);
    const out = suggestOrientations(topo, o);
    cases.suggestOrientations.push({ id: `${name}|top${o.top}|t${o.threshold}`, args: [topoRef(name), o, new Ref({ $config: encode(snap()) })], out });
  }
}

for (const [fn, cs] of Object.entries(cases)) writeCases("orient", fn, cs);
console.log("orient:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])));
