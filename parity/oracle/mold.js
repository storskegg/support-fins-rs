// Records parity/corpus/mold/{floorLine,moldLine}.jsonl (direct) and *.captured.jsonl from web/prop/mold.js.
// floorLine  args: [topLine ([x,y,z]...), tris, margin, mold, {$config: {PROP}}]   out: [[x,y,z] | [x,y,z,zNeg,zPos]...]
// moldLine   args: [top, tris, {$config: {PROP}}]                                   out: {top, floor}
// `tris` is the whole seated part ($seated). margin/mold are written out (JS defaults 1.0 / false).
import { writeCases } from "./lib/encode.js";
import { cfgRef, regionsOf, REGION_MODELS, REGION_POSES, seatedRef, withConfig } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const { floorLine, moldLine } = await import(`${WEB}prop/mold.js`);
const { contactLine } = await import(`${WEB}prop/contact.js`);

const cases = { floorLine: [], moldLine: [] };
for (const name of REGION_MODELS) {
  for (const deg of REGION_POSES) {
    for (const r of regionsOf(name, deg, 2)) {
      const line = contactLine(r.pts, r.soup, 16);
      if (!line) continue;
      const ref = seatedRef(name, r.rot, r.offset);
      const tag = `${name}|r${deg.join(",")}|g${r.ri}`;
      for (const footGap of [0.2, 0]) {
        withConfig({ PROP: { footGap } }, () => {
          for (const margin of [1.0, 0.25]) {
            for (const mold of [false, true]) {
              const id = `${tag}|fg${footGap}|m${margin}|${mold ? "mold" : "plain"}`;
              cases.floorLine.push({ id, args: [line, ref, margin, mold, cfgRef("PROP")], out: floorLine(line, r.soup, margin, mold) });
            }
          }
          cases.moldLine.push({ id: `${tag}|fg${footGap}`, args: [line, ref, cfgRef("PROP")], out: moldLine(line, r.soup) });
        });
      }
    }
  }
}
// an empty and a one-station line
for (const line of [[], [[0, 0, 5]]]) {
  cases.floorLine.push({ id: `short${line.length}`, args: [line, [], 1.0, false, cfgRef("PROP")], out: floorLine(line, new Float64Array(0), 1.0, false) });
}
for (const [fn, list] of Object.entries(cases)) writeCases("mold", fn, list);

await recordGolden("mold", [{ rel: "prop/mold.js", fns: ["floorLine", "moldLine"] }], {
  config: { floorLine: ["PROP"], moldLine: ["PROP"] },
  configs: async (cap) => {
    const m = await import(new URL("web/prop/config.js", cap.rootUrl).href);
    return { PROP: () => m.PROP };
  },
});
console.log("mold:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])));
