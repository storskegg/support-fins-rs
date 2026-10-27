// Records parity/corpus/contact/{straightness,contactLine,lowerSag,contourTop,settleTop}.jsonl (direct) and
// *.captured.jsonl from web/prop/contact.js.
// straightness  args: [line ([x,y,z]...)]                                  out: number
// contactLine   args: [pts, tris, nSamples, {$config: {PROP}}]             out: line | null
// lowerSag      args: [line, tris, band]                                   out: the (mutated) line
// contourTop    args: [line, tris, band, {$config: {PROP}}]                out: the (mutated) line
// settleTop     args: [line, tris, step, band, {$config: {PROP}}]          out: the (mutated) line
// `line` is recorded as it was BEFORE the call (the functions edit it in place). `tris` is a region's seated
// soup (Float64Array) or the seated part. A captured call records the arguments it passed (the trailing
// $config follows them); the JS defaults (band = Infinity, step = 0.25) fill the rest on replay.
import { writeCases } from "./lib/encode.js";
import { cfgRef, REGION_MODELS, REGION_POSES, regionsOf } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const { straightness, contactLine, lowerSag, contourTop, settleTop } = await import(`${WEB}prop/contact.js`);

const copy = (line) => line.map((p) => p.slice());
const cases = { straightness: [], contactLine: [], lowerSag: [], contourTop: [], settleTop: [] };

const ring = Array.from({ length: 24 }, (_, i) => [10 * Math.cos((i / 24) * 2 * Math.PI), 10 * Math.sin((i / 24) * 2 * Math.PI), 3]);
const straight = Array.from({ length: 10 }, (_, i) => [i * 2, i * 0.01, 1]);
for (const [tag, line] of Object.entries({ ring, straight, two: [[0, 0, 0], [1, 1, 1]], empty: [], same: [[1, 1, 1], [1, 1, 2], [1, 1, 3]], zigzag: [[0, 0, 0], [5, 1, 0], [10, -1, 0], [15, 1, 0]] })) {
  cases.straightness.push({ id: tag, args: [line], out: straightness(line) });
}

for (const name of REGION_MODELS) {
  for (const deg of REGION_POSES) {
    for (const r of regionsOf(name, deg, 3)) {
      const tag = `${name}|r${deg.join(",")}|g${r.ri}`;
      const tris = r.tris;
      for (const ns of [8, 24]) {
        cases.contactLine.push({ id: `${tag}|n${ns}`, args: [r.pts, tris, ns, cfgRef("PROP")], out: contactLine(r.pts, tris, ns) });
      }
      const base = contactLine(r.pts, tris, 16);
      if (!base) continue;
      cases.straightness.push({ id: tag, args: [base], out: straightness(base) });
      // lines that sit proud of the surface (+0.3) so every pass has something to do
      for (const lift of [0.3, 0.05]) {
        const proud = base.map((p) => [p[0], p[1], p[2] + lift]);
        for (const band of [Infinity, 0.5]) {
          const id = `${tag}|lift${lift}|band${band}`;
          cases.lowerSag.push({ id, args: [copy(proud), tris, band], out: lowerSag(copy(proud), tris, band) });
          cases.contourTop.push({ id, args: [copy(proud), tris, band, cfgRef("PROP")], out: contourTop(copy(proud), tris, band) });
          for (const step of [0.25, 1]) {
            const sid = `${id}|step${step}`;
            cases.settleTop.push({ id: sid, args: [copy(proud), tris, step, band, cfgRef("PROP")], out: settleTop(copy(proud), tris, step, band) });
          }
        }
      }
    }
  }
}
// edge inputs: fewer than 3 points, a degenerate (one-point) spread, a flat line
{
  const { tris } = regionsOf("cube", [40, 0, 0], 1)[0];
  const same = [[1, 1, 1], [1, 1, 1], [1, 1, 1]];
  for (const [tag, pts] of Object.entries({ two: [[0, 0, 0], [1, 0, 0]], same })) {
    cases.contactLine.push({ id: `edge-${tag}`, args: [pts, tris, 8, cfgRef("PROP")], out: contactLine(pts, tris, 8) });
  }
}
for (const [fn, list] of Object.entries(cases)) writeCases("contact", fn, list);

await recordGolden("contact", [{ rel: "prop/contact.js", fns: Object.keys(cases) }], {
  config: { contactLine: ["PROP"], contourTop: ["PROP"], settleTop: ["PROP"] },
  configs: async (cap) => {
    const m = await import(new URL("web/prop/config.js", cap.rootUrl).href);
    return { PROP: () => m.PROP };
  },
});
console.log("contact:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])));
