// Records parity/corpus/clearance/*.jsonl (direct) and *.captured.jsonl from web/prop/clearance.js.
// Lines are [x, y, z] stations; the part is ($topo, rot, offset); configs trail as {$config: {PROP}}.
// stationIsClear / stationCertified  args: [line, k, $topo, rot, offset, $config]   out: boolean
// pathToPlateIsClear                 args: [line, $topo, rot, offset, $config]      out: boolean
// welds                              args: [{d, cosUp}, $config]                    out: boolean
// longestRun                         args: [usable]                                 out: [a, b] | null
// withLowTails                       args: [tall, low]                              out: [boolean...]
// bodyMask                           args: [pts, $config]                           out: [boolean...]
// tallBody                           args: [mask]                                   out: [a, b] | null
// tallSpan                           args: [pts, mask]                              out: number
// minSpanFor                         args: [pts, mask, smallTube, short, $config]   out: number
// insertFloorStations                args: [line, $config]                          out: the (mutated) line
// A captured call records the arguments it passed; JS defaults (smallTube = short = false) fill the rest.
import { writeCases } from "./lib/encode.js";
import { cfgRef, REGION_MODELS, REGION_POSES, regionsOf, topoRef, withConfig } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { WEB } from "./lib/pin.js";

const C = await import(`${WEB}prop/clearance.js`);
const { contactLine } = await import(`${WEB}prop/contact.js`);

const names = ["stationIsClear", "stationCertified", "welds", "pathToPlateIsClear", "longestRun", "withLowTails", "bodyMask", "tallBody", "tallSpan", "minSpanFor", "insertFloorStations"];
const cases = Object.fromEntries(names.map((n) => [n, []]));
const copy = (l) => l.map((p) => p.slice());

for (const name of REGION_MODELS) {
  for (const deg of REGION_POSES) {
    for (const r of regionsOf(name, deg, 2)) {
      const line = contactLine(r.pts, r.soup, 20);
      if (!line) continue;
      const tag = `${name}|r${deg.join(",")}|g${r.ri}`;
      const part = [topoRef(name), r.rot, r.offset];
      for (let k = 0; k < line.length; k += 2) {
        cases.stationIsClear.push({ id: `${tag}|k${k}`, args: [line, k, ...part, cfgRef("PROP")], out: C.stationIsClear(line, k, r.topo, r.rot, r.offset) });
        cases.stationCertified.push({ id: `${tag}|k${k}`, args: [line, k, ...part, cfgRef("PROP")], out: C.stationCertified(line, k, r.topo, r.rot, r.offset) });
      }
      // lowered a bit: stations that start to clip the part
      const low = line.map((p) => [p[0], p[1], p[2] + 3]);
      for (const [vtag, l] of Object.entries({ line, up3: low })) {
        cases.pathToPlateIsClear.push({ id: `${tag}|${vtag}`, args: [l, ...part, cfgRef("PROP")], out: C.pathToPlateIsClear(l, r.topo, r.rot, r.offset) });
      }
      const mask = C.bodyMask(line);
      cases.bodyMask.push({ id: tag, args: [line, cfgRef("PROP")], out: mask });
      cases.tallBody.push({ id: tag, args: [mask], out: C.tallBody(mask) });
      cases.tallSpan.push({ id: tag, args: [line, mask], out: C.tallSpan(line, mask) });
      for (const [st, sh] of [[false, false], [true, false], [false, true]]) {
        cases.minSpanFor.push({ id: `${tag}|t${st}s${sh}`, args: [line, mask, st, sh, cfgRef("PROP")], out: C.minSpanFor(line, mask, st, sh) });
      }
      // stations that dip under the squat floor at either end
      const dipped = line.map((p, i) => [p[0], p[1], i < 3 ? 0.1 + 0.1 * i : (i >= line.length - 3 ? 0.05 + 0.2 * (line.length - 1 - i) : p[2])]);
      for (const [vtag, l] of Object.entries({ line, dipped })) {
        cases.insertFloorStations.push({ id: `${tag}|${vtag}`, args: [copy(l), cfgRef("PROP")], out: C.insertFloorStations(copy(l)) });
      }
    }
  }
}

// welds: a grid over d and cosUp, at PLA and PETG gaps
for (const gap of [0.2, 0.3]) {
  withConfig({ PROP: { gap } }, () => {
    for (const cosUp of [-1, 0, 0.5, 0.7, 0.7000001, 0.9, 1]) {
      for (const d of [0, 0.1, 0.134, 0.135, 0.1351, 0.2, 0.204, 0.205, 0.2051, 0.2025, 0.23, 0.45]) {
        const hit = { d, cosUp };
        cases.welds.push({ id: `gap${gap}|c${cosUp}|d${d}`, args: [hit, cfgRef("PROP")], out: C.welds(hit) });
      }
    }
  });
}

// longestRun / withLowTails: every pattern up to 6 stations (and a few longer), tall/low combinations
const bools = (n, bits) => Array.from({ length: n }, (_, i) => !!((bits >> i) & 1));
for (let n = 0; n <= 6; n++) {
  for (let bits = 0; bits < 1 << n; bits++) {
    const u = bools(n, bits);
    cases.longestRun.push({ id: `n${n}b${bits}`, args: [u], out: C.longestRun(u) });
  }
}
cases.longestRun.push({ id: "tie", args: [[true, true, false, true, true]], out: C.longestRun([true, true, false, true, true]) });
for (let n = 1; n <= 5; n++) {
  for (let t = 0; t < 1 << n; t++) {
    for (let l = 0; l < 1 << n; l += 3) {
      const tall = bools(n, t), lowm = bools(n, l | t); // tall stations are low too, as in buildProps
      cases.withLowTails.push({ id: `n${n}t${t}l${l}`, args: [tall, lowm], out: C.withLowTails(tall, lowm) });
    }
  }
}
// tallBody / tallSpan on masks
for (let n = 0; n <= 5; n++) {
  for (let bits = 0; bits < 1 << n; bits++) {
    const m = bools(n, bits);
    cases.tallBody.push({ id: `m${n}b${bits}`, args: [m], out: C.tallBody(m) });
  }
}
// short lines for insertFloorStations
withConfig({ PROP: {} }, () => {
  for (const [tag, l] of Object.entries({ empty: [], one: [[0, 0, 5]], allLow: [[0, 0, 0], [1, 0, 0.1]], allHigh: [[0, 0, 5], [1, 0, 6]], bothEnds: [[0, 0, 0.1], [1, 0, 0.3], [2, 0, 5], [3, 0, 5], [4, 0, 0.3], [5, 0, 0.1]] })) {
    cases.insertFloorStations.push({ id: `synthetic-${tag}`, args: [copy(l), cfgRef("PROP")], out: C.insertFloorStations(copy(l)) });
  }
});

for (const [fn, list] of Object.entries(cases)) writeCases("clearance", fn, list);

await recordGolden("clearance", [{ rel: "prop/clearance.js", fns: names }], {
  config: { stationIsClear: ["PROP"], stationCertified: ["PROP"], welds: ["PROP"], pathToPlateIsClear: ["PROP"], bodyMask: ["PROP"], minSpanFor: ["PROP"], insertFloorStations: ["PROP"] },
  configs: async (cap) => {
    const m = await import(new URL("web/prop/config.js", cap.rootUrl).href);
    return { PROP: () => m.PROP };
  },
});
console.log("clearance:", Object.fromEntries(Object.entries(cases).map(([k, v]) => [k, v.length])));
