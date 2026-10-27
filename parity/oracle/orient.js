// Records parity/corpus/orient/*.jsonl (web/orient.js; direct cases).
//   layerVerdict         args: [{x, y, z}]                    out: {posture}
//   loadAlignment        args: [[dx, dy, dz]]                 out: {cross, quality, text} | null
//   suggestStrengthPose  args: [$topo, [dx, dy, dz], {threshold}]
//                        out: {rot, cross, quality, text, height, over, bedArea} | null
// suggestOrientations is recorded by the buildFins PR (it builds fins on its finalists).
// Replayed by crates/sf-testkit/tests/parity_orient.rs.
import { writeCases } from "./lib/encode.js";
import { modelList, topoRef, topoOf } from "./lib/direct.js";
import { WEB } from "./lib/pin.js";

const { layerVerdict, loadAlignment, suggestStrengthPose } = await import(`${WEB}orient.js`);

const cases = { layerVerdict: [], loadAlignment: [], suggestStrengthPose: [] };

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

for (const [fn, cs] of Object.entries(cases)) writeCases("orient", fn, cs);
console.log("orient:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])));
