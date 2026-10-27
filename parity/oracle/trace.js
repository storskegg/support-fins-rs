// Records parity/corpus/trace/traceGroups.jsonl (direct) and traceGroups.captured.jsonl from web/prop/trace.js.
// traceGroups  args: [$topo, $analysis, rot, off, Set(tried region indices), {$config: {TRACE}}]
//              out: {leaderOf: Map(region -> leader), groups: Map(leader -> {members, tris, pts, red})}
// (`result` may be a spread copy of the analyze result carrying excludedSupportFaces: then it is recorded in full.)
import { writeCases } from "./lib/encode.js";
import { analysisRef, cfgRef, modelList, posed, topoRef } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const { traceGroups } = await import(`${WEB}prop/trace.js`);

const cases = [];
const POSES = [[40, 0, 0], [0, 35, 20], [90, 0, 0], [0, 0, 0], [-45, 30, 15]];
for (const [name] of modelList()) {
  for (const deg of POSES) {
    const { topo, rot, res, offset } = posed(name, deg);
    const n = res.regions.length;
    if (!n) continue;
    const all = Array.from({ length: Math.min(n, 8) }, (_, i) => i);
    const sets = { all, first: [0], ...(n > 2 ? { odd: all.filter((i) => i % 2 === 1), rev: [...all].reverse() } : {}) };
    for (const [tag, list] of Object.entries(sets)) {
      if (!list.length) continue;
      const tried = new Set(list);
      cases.push({
        id: `${name}|r${deg.join(",")}|${tag}`,
        args: [topoRef(name), analysisRef(name, 45, rot), rot, offset, new Set(tried), cfgRef("TRACE")],
        out: traceGroups(topo, res, rot, offset, tried),
      });
    }
  }
}
writeCases("trace", "traceGroups", cases);

await recordGolden("trace", [{ rel: "prop/trace.js", fns: ["traceGroups"] }], {
  config: { traceGroups: ["TRACE"] },
  configs: async (cap) => {
    const m = await import(new URL("web/prop/trace.js", cap.rootUrl).href);
    return { TRACE: () => m.TRACE };
  },
});
console.log("trace:", cases.length);
