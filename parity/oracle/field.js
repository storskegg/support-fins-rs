// Records parity/corpus/field/headingField.jsonl from web/fins/field.js (direct only: the JS returns a closure).
// headingField  args: [$topo, $analysis, rot, points ([x, y]...), {$config: {FIELD}}]
//               out: [radians | null ...], the closure applied to each point
import { writeCases } from "./lib/encode.js";
import { analysisRef, cfgRef, modelList, posed, POSES, topoRef } from "./lib/direct.js";
import { WEB } from "./lib/pin.js";

const { headingField } = await import(`${WEB}fins/field.js`);

const cases = [];
for (const [name] of modelList()) {
  for (const deg of POSES) {
    const { topo, rot, res, offset } = posed(name, deg);
    const field = headingField(topo, res, rot);
    const w = res.size.x, h = res.size.y;
    const pts = [];
    for (const fx of [-0.3, 0, 0.2, 0.5, 0.8, 1, 1.4]) for (const fy of [-0.3, 0.1, 0.5, 0.9, 1.3]) {
      pts.push([(fx - 0.5) * w, (fy - 0.5) * h]);
    }
    pts.push([1e4, 1e4], [0, 0]);
    cases.push({
      id: `${name}|r${deg.join(",")}`,
      args: [topoRef(name), analysisRef(name, 45, rot), rot, pts, cfgRef("FIELD")],
      out: pts.map(([x, y]) => field(x, y)),
    });
  }
}
writeCases("field", "headingField", cases);
console.log("field:", cases.length, "cases;", cases.flatMap((c) => c.out).filter((v) => v !== null).length, "headings");
