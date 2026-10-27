// Records parity/corpus/arc/arcLine.jsonl (direct) and arcLine.captured.jsonl from web/prop/arc.js.
// arcLine  args: [regionPts ([x,y,z]...), regionTris (Float64Array), step, {$config: {PROP, ARC, CURVED}}]
//          out: [[[x,y,z]...]] | null
// Direct inputs: synthetic annular strips (a slot in a tube's wall) in every way the header lists --
// a good arc, too wide, a full ring, tilted, nearly straight, too small, a radius under the probe reach --
// plus the regions of every model/pose.
import { writeCases } from "./lib/encode.js";
import { cfgRef, modelList, regionsOf, withConfig } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const { arcLine } = await import(`${WEB}prop/arc.js`);
const cases = [];

/** An annular strip: radii r0..r1 round (cx, cy), angles a0..a1, at height z (+ rise across), `n` facets. */
function strip({ cx = 0, cy = 0, r0, r1, a0 = 0, a1, z = 5, rise = 0, n = 24 }) {
  const P = (r, a) => [cx + r * Math.cos(a), cy + r * Math.sin(a), z + (rise * (r - r0)) / (r1 - r0)];
  const tris = [], pts = [];
  for (let i = 0; i < n; i++) {
    const s = a0 + ((a1 - a0) * i) / n, e = a0 + ((a1 - a0) * (i + 1)) / n;
    const q = [P(r0, s), P(r1, s), P(r1, e), P(r0, e)];
    for (const t of [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]) {
      tris.push(...t.flat());
      pts.push(...t);
      pts.push([(t[0][0] + t[1][0] + t[2][0]) / 3, (t[0][1] + t[1][1] + t[2][1]) / 3, (t[0][2] + t[1][2] + t[2][2]) / 3]);
    }
  }
  return { pts, tris: new Float64Array(tris) };
}
const deg = (d) => (d * Math.PI) / 180;
const synth = {
  slot: { r0: 14.2, r1: 15.8, a1: deg(90) },
  slotBig: { r0: 29, r1: 31, a1: deg(200), n: 60 },
  wide: { r0: 14, r1: 22, a1: deg(90) },
  ring: { r0: 14.2, r1: 15.8, a1: deg(360), n: 72 },
  ringGap20: { r0: 14.2, r1: 15.8, a1: deg(340), n: 72 },
  ringGap40: { r0: 14.2, r1: 15.8, a1: deg(320), n: 72 },
  tilted: { r0: 14.2, r1: 15.8, a1: deg(90), rise: 2 },
  nearlyFlat: { r0: 14.2, r1: 15.8, a1: deg(90), rise: 0.8 },
  straightish: { r0: 400, r1: 401.6, a1: deg(1), n: 12 },
  short: { r0: 14.2, r1: 15.8, a1: deg(1), n: 2 },
  smallRadius: { r0: 3, r1: 4.6, a1: deg(90) },
  offOrigin: { cx: 40, cy: -25, r0: 10, r1: 11.6, a0: deg(100), a1: deg(250), z: 2 },
  seam: { r0: 14.2, r1: 15.8, a0: deg(-170), a1: deg(-20) },
};
for (const [tag, spec] of Object.entries(synth)) {
  const { pts, tris } = strip(spec);
  for (const step of [1, 0.5]) {
    cases.push({ id: `synth|${tag}|s${step}`, args: [pts, tris, step, cfgRef("PROP", "ARC", "CURVED")], out: arcLine(pts, tris, step) });
  }
}
// ARC / CURVED tunables moved
{
  const { pts, tris } = strip(synth.slot);
  for (const [tag, over] of Object.entries({ maxWidth1: { ARC: { maxWidth: 1 } }, minLen99: { ARC: { minLen: 99 } }, minBend: { CURVED: { minBend: 360 } }, minGap80: { ARC: { minGap: 80 } } })) {
    withConfig(over, () => cases.push({ id: `tune|${tag}`, args: [pts, tris, 1, cfgRef("PROP", "ARC", "CURVED")], out: arcLine(pts, tris, 1) }));
  }
  cases.push({ id: "few", args: [pts.slice(0, 5), tris, 1, cfgRef("PROP", "ARC", "CURVED")], out: arcLine(pts.slice(0, 5), tris, 1) });
}
for (const [name] of modelList()) {
  for (const deg_ of [[0, 0, 0], [40, 0, 0], [90, 0, 0], [0, 35, 20]]) {
    for (const r of regionsOf(name, deg_, 3)) {
      cases.push({ id: `${name}|r${deg_.join(",")}|g${r.ri}`, args: [r.pts, r.tris, 1, cfgRef("PROP", "ARC", "CURVED")], out: arcLine(r.pts, r.tris, 1) });
    }
  }
}
writeCases("arc", "arcLine", cases);

await recordGolden("arc", [{ rel: "prop/arc.js", fns: ["arcLine"] }], {
  config: { arcLine: ["PROP", "ARC", "CURVED"] },
  configs: async (cap) => {
    const p = await import(new URL("web/prop/config.js", cap.rootUrl).href);
    const a = await import(new URL("web/prop/arc.js", cap.rootUrl).href);
    const c = await import(new URL("web/prop/curved.js", cap.rootUrl).href);
    return { PROP: () => p.PROP, ARC: () => a.ARC, CURVED: () => c.CURVED };
  },
});
console.log("arc:", cases.length, "cases;", cases.filter((c) => c.out !== null).length, "non-null");
