// Records parity/corpus/surface/{surfaceZAt,surfaceHitsAt,surfaceZsAt}.jsonl (direct) and
// *.captured.jsonl (calls a golden-scene build makes), from web/prop/surface.js.
// All three take args: [tris, x, y]; `tris` is the seated part soup, recorded as a $seated
// reference. out: surfaceZAt a number or null; surfaceHitsAt [[z, nz]...]; surfaceZsAt [z...].
// `seat` (rot, offset, vertex) is pure arithmetic and is checked by the seating cases.
import { writeCases } from "./lib/encode.js";
import { extentXY, FRACS, modelList, posed, POSES, seatedRef } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const { surfaceZAt, surfaceHitsAt, surfaceZsAt } = await import(`${WEB}prop/surface.js`);

const FNS = { surfaceZAt, surfaceHitsAt, surfaceZsAt };
const cases = { surfaceZAt: [], surfaceHitsAt: [], surfaceZsAt: [] };

function columns(name, deg, fracs) {
  const { rot, offset, soup } = posed(name, deg);
  const e = extentXY(soup);
  const ref = seatedRef(name, rot, offset);
  const tag = `${name}|r${deg.join(",")}`;
  for (const fx of fracs) {
    for (const fy of fracs) {
      const x = e.x0 + fx * (e.x1 - e.x0), y = e.y0 + fy * (e.y1 - e.y0);
      for (const [fn, f] of Object.entries(FNS)) {
        cases[fn].push({ id: `${tag}|${x},${y}`, args: [ref, x, y], out: f(soup, x, y) });
      }
    }
  }
}

for (const [name] of modelList()) {
  for (const deg of POSES) columns(name, deg, FRACS);
}

// A dense sweep over one soup: past LOW_FIRST_AFTER queries the grid sorts its lists by floor, and the
// answers must not change. (Replayed in file order, the same soup keeps its grid.)
{
  const { rot, offset, soup } = posed("cube", [40, 0, 0]);
  const e = extentXY(soup);
  const ref = seatedRef("cube", rot, offset);
  const N = 60;
  for (let i = 0; i < N; i++) {
    for (let j = 0; j < N; j++) {
      const x = e.x0 + (i / (N - 1)) * (e.x1 - e.x0), y = e.y0 + (j / (N - 1)) * (e.y1 - e.y0);
      cases.surfaceZAt.push({ id: `dense|${x},${y}`, args: [ref, x, y], out: surfaceZAt(soup, x, y) });
    }
  }
}

for (const [fn, list] of Object.entries(cases)) writeCases("surface", fn, list);

await recordGolden("surface", [{ rel: "prop/surface.js", fns: ["surfaceZAt", "surfaceHitsAt", "surfaceZsAt"] }]);
console.log("surface:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])));
