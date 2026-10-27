// Records parity/corpus/rim/{rimTracks,clipToRim}.jsonl (direct) and *.captured.jsonl from web/prop/rim.js.
// rimTracks  args: [$topo, $analysis, rot, faces, regionTris, step, {$config: {PROP, RIM}}]
//            out: [[[x,y,z]...]...] (each track is tagged `rim: true` in JS; the tag is not recorded)
// clipToRim  args: [rows, rim, {$config: {PROP}}]   out: [[[x,y,z]...]...]
// A captured call records the arguments it passed; JS defaults (step = PROP.stationStep) fill the rest.
import { writeCases } from "./lib/encode.js";
import { analysisRef, cfgMods, cfgRef, modelList, posed, topoRef, withConfig } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const { rimTracks, clipToRim } = await import(`${WEB}prop/rim.js`);
const cases = { rimTracks: [], clipToRim: [] };

const POSES = [[0, 0, 0], [40, 0, 0], [90, 0, 0], [0, 35, 20], [180, 0, 0], [-45, 30, 15]];
function regionTrisOf(topo, rot, offset, faces) {
  const tris = new Float64Array(faces.length * 9);
  faces.forEach((f, k) => {
    for (let i = 0; i < 3; i++) {
      const o = f * 9 + i * 3, x = topo.pos[o], y = topo.pos[o + 1], z = topo.pos[o + 2];
      tris[k * 9 + i * 3] = rot[0] * x + rot[3] * y + rot[6] * z + offset.x;
      tris[k * 9 + i * 3 + 1] = rot[1] * x + rot[4] * y + rot[7] * z + offset.y;
      tris[k * 9 + i * 3 + 2] = rot[2] * x + rot[5] * y + rot[8] * z + offset.z;
    }
  });
  return tris;
}

let nonEmpty = 0;
for (const [name] of modelList()) {
  for (const deg of POSES) {
    const { topo, rot, res, offset } = posed(name, deg);
    for (const [gi, region] of res.regions.slice(0, 4).entries()) {
      const tris = regionTrisOf(topo, rot, offset, region.faces);
      const tag = `${name}|r${deg.join(",")}|g${gi}`;
      const refs = [topoRef(name), analysisRef(name, 45, rot), rot, region.faces];
      for (const [stag, step, over] of [["s1", 1, {}], ["s05", 0.5, {}], ["short", 1, { pieceLen: 6, pieceTurn: 30, pieceGap: 2.5 }], ["lvl45", 1, { levelDeg: 45 }]]) {
        withConfig({ RIM: over }, () => {
          const out = rimTracks(topo, res, rot, region.faces, tris, step);
          if (out.length) nonEmpty++;
          cases.rimTracks.push({ id: `${tag}|${stag}`, args: [...refs, tris, step, cfgRef("PROP", "RIM")], out });
          // rows across the region, clipped to its rim
          const rows = [];
          if (out.length) {
            const xs = tris.filter((_, i) => i % 3 === 0), ys = tris.filter((_, i) => i % 3 === 1);
            const x0 = Math.min(...xs), x1 = Math.max(...xs), y0 = Math.min(...ys), y1 = Math.max(...ys);
            for (let y = y0 + 1; y < y1; y += 6) {
              const row = [];
              for (let x = x0; x <= x1; x += 1) row.push([x, y, 5]);
              rows.push(row);
            }
            cases.clipToRim.push({ id: `${tag}|${stag}`, args: [rows, out, cfgRef("PROP")], out: clipToRim(rows, out) });
          }
        });
      }
    }
  }
}
// clipToRim edges: no rim returns the rows as they are; empty rows; a row wholly inside the clearance
cases.clipToRim.push({ id: "norim", args: [[[[0, 0, 1], [1, 0, 1]]], [], cfgRef("PROP")], out: clipToRim([[[0, 0, 1], [1, 0, 1]]], []) });
cases.clipToRim.push({ id: "norows", args: [[], [[[0, 0, 1], [1, 0, 1], [2, 0, 1]]], cfgRef("PROP")], out: clipToRim([], [[[0, 0, 1], [1, 0, 1], [2, 0, 1]]]) });
{
  const rim = [Array.from({ length: 12 }, (_, i) => [i, 0, 2])];
  const rows = [Array.from({ length: 12 }, (_, i) => [i, 0.5, 2]), Array.from({ length: 12 }, (_, i) => [i, 6, 2]), [[0, 3, 1], [1, 3, 1]]];
  cases.clipToRim.push({ id: "inside", args: [rows, rim, cfgRef("PROP")], out: clipToRim(rows, rim) });
}
for (const [fn, list] of Object.entries(cases)) writeCases("rim", fn, list);

await recordGolden("rim", [{ rel: "prop/rim.js", fns: ["rimTracks", "clipToRim"] }], {
  config: { rimTracks: ["PROP", "RIM"], clipToRim: ["PROP"] },
  configs: async (cap) => {
    const p = await import(new URL("web/prop/config.js", cap.rootUrl).href);
    const r = await import(new URL("web/prop/rim.js", cap.rootUrl).href);
    return { PROP: () => p.PROP, RIM: () => r.RIM };
  },
});
console.log("rim:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])), "non-empty rimTracks:", nonEmpty);
