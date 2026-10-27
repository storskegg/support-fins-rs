// Records parity/corpus/downhill/downhillLines.jsonl (direct) and downhillLines.captured.jsonl from
// web/prop/downhill.js.
// downhillLines  args: [regionPts, regionTris, step, pitch, o, {$config: {PROP, DOWNHILL}}]
//                o = {h?, cover?, reach?}   out: [[[x,y,z]...]...] | null
// A captured call records the arguments it passed; the JS defaults (step = PROP.stationStep,
// pitch = PROP.maxUnsupportedSpan, o = {}) fill the rest on replay.
import { writeCases } from "./lib/encode.js";
import { cfgMods, cfgRef, REGION_MODELS, REGION_POSES, regionsOf, modelList, posed } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const { downhillLines } = await import(`${WEB}prop/downhill.js`);
const { PROP } = cfgMods;

const cases = [];
const models = [...new Set([...REGION_MODELS, ...modelList().map(([n]) => n).filter((n) => !n.startsWith("fixtures/"))])];
for (const name of models) {
  for (const deg of [...REGION_POSES, [-45, 30, 15]]) {
    for (const r of regionsOf(name, deg, 3)) {
      const tag = `${name}|r${deg.join(",")}|g${r.ri}`;
      const step = PROP.stationStep, pitch = PROP.maxUnsupportedSpan;
      const run = (id, st, pi, o) => {
        cases.push({ id, args: [r.pts, r.tris, st, pi, o, cfgRef("PROP", "DOWNHILL")], out: downhillLines(r.pts, r.tris, st, pi, o) });
      };
      run(`${tag}|plain`, step, pitch, {});
      run(`${tag}|fine`, 0.5, 5, {});
      // a grown-surface call: red to cover, a coarse slope step, a short reach
      const red = r.pts.filter((_, i) => i % 4 === 3);
      run(`${tag}|cover`, step, pitch, { h: 2, cover: red, reach: pitch / 2 });
      run(`${tag}|cover-reach`, step, pitch, { cover: red, reach: 3 });
    }
  }
}
writeCases("downhill", "downhillLines", cases);

await recordGolden("downhill", [{ rel: "prop/downhill.js", fns: ["downhillLines"] }], {
  config: { downhillLines: ["PROP", "DOWNHILL"] },
  configs: async (cap) => {
    const p = await import(new URL("web/prop/config.js", cap.rootUrl).href);
    const d = await import(new URL("web/prop/downhill.js", cap.rootUrl).href);
    return { PROP: () => p.PROP, DOWNHILL: () => d.DOWNHILL };
  },
});
console.log("downhill:", cases.length, cases.filter((c) => c.out !== null).length, "non-null");
