// Records parity/corpus/seating/{seatedPartTris,bedContact,seatingOf}.jsonl (direct) and *.captured.jsonl
// from web/fins/seating.js.
// seatedPartTris  args: [$topo, rot, offset]            out: the soup (Float64Array)
// bedContact      args: [$topo, $analysis, rot]         out: {pts: [[x, y]...], mx, my}
// seatingOf       args: [$analysis, pts, {$config: {FIN}}]   out: {kind, span, bedArea}
//                 (`result` is an $analysis reference, or a plain {bedArea} in the synthetic cases: seatingOf
//                 reads only result.bedArea)
import { writeCases, Ref, encode } from "./lib/encode.js";
import { analysisRef, modelList, posed, POSES, topoRef } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const { bedContact, seatingOf } = await import(`${WEB}fins/seating.js`);
const { FIN } = await import(`${WEB}fins/config.js`);
const finRef = () => new Ref({ $config: { FIN: encode(FIN) } });

const cases = { seatedPartTris: [], bedContact: [], seatingOf: [] };
const stress = new Set(modelList().filter(([n]) => !n.startsWith("fixtures/")).map(([n]) => n));

for (const [name] of modelList()) {
  for (const [i, deg] of POSES.entries()) {
    const { topo, rot, res, offset, soup } = posed(name, deg);
    const id = `${name}|r${deg.join(",")}`;
    if (i < (stress.has(name) ? 3 : 1)) {
      cases.seatedPartTris.push({ id, args: [topoRef(name), rot, offset], out: soup });
    }
    const bc = bedContact(topo, res, rot);
    cases.bedContact.push({ id, args: [topoRef(name), analysisRef(name, 45, rot), rot], out: bc });
    cases.seatingOf.push({ id, args: [analysisRef(name, 45, rot), bc.pts, finRef()], out: seatingOf(res, bc.pts) });
  }
}
// seatingOf's three kinds and the empty contact (not every model reaches 'point')
{
  const { res } = posed("cube", [0, 0, 0]);
  const fake = (bedArea) => ({ ...res, bedArea });
  for (const [tag, bedArea, pts] of [
    ["face", 100, [[0, 0], [10, 0]]], ["point", 0, [[0, 0], [1, 1]]], ["edge", 0, [[0, 0], [10, 0]]],
    ["empty", 0, []], ["exact-60", 60, []], ["just-under", 59.999, [[0, 0], [2, 0]]],
  ]) {
    cases.seatingOf.push({ id: `synthetic-${tag}`, args: [{ bedArea }, pts, finRef()], out: seatingOf(fake(bedArea), pts) });
  }
}
for (const [fn, list] of Object.entries(cases)) writeCases("seating", fn, list);

await recordGolden("seating", [{ rel: "fins/seating.js", fns: ["seatedPartTris", "bedContact", "seatingOf"] }], {
  config: { seatingOf: ["FIN"] },
  configs: async (cap) => {
    const m = await import(new URL("web/fins/config.js", cap.rootUrl).href);
    return { FIN: () => m.FIN };
  },
});
console.log("seating:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])));
