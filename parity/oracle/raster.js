// Records parity/corpus/raster/*.jsonl (web/prop/raster.js).
//   gripRise      args: [z]                                              out: number
//   rasterRest    args: [line, run, {$config: {PROP}}]                   out: [{ pts, from }]
//   rasterTracks  args: [regionPts, regionTris, partTris($seated), step, rowSpan, support, {$config: {PROP}}]
//                 support: {topo: {$topo}, rot, offset, sway}            out: { lines, spacing }
//   raceAgainst   args: [$topo, $analysis, rot, scenario, {$config: {PROP}}]
//                 raceAgainst takes a pass-building closure and returns buildProps's shape, so each case is a
//                 SCENARIO: the normal pass and the second pass are scripted (walls with centrelines near the
//                 real overhang faces, scripted triangles, tines and tine-capture slices), the closure returns
//                 the second pass whatever regions it is asked for (they are recorded), and the tests' tine
//                 capture is set up when `scenario.cap` is not null.
//                 scenario: { tag, maxGrow, rel, lenient, equivalent, tried, ruler, price, veto, capFrom, cap,
//                             normal, second }
//                 out: { triangles, props, served, servedRegions, tines, sagRisk, volume, swapped, sagRegions,
//                        asked, cap, sameAsNormal }
// Replayed by crates/sf-testkit/tests/parity_raster.rs.
import { encode, writeCases } from "./lib/encode.js";
import { cfgMods, cfgRef, REGION_MODELS, REGION_POSES, posed, regionsOf, seatedRef, topoRef, analysisRef } from "./lib/direct.js";
import { recordGolden } from "./lib/golden.js";
import { assertPinned, WEB } from "./lib/pin.js";

assertPinned();
const { gripRise, rasterRest, rasterTracks, raceAgainst } = await import(`${WEB}prop/raster.js`);
const PROP = cfgMods.PROP;

const cases = { gripRise: [], rasterRest: [], rasterTracks: [], raceAgainst: [] };

for (const z of [0, 1, 2, 10, 40, 80, 123.456, -3, Infinity, NaN, 1e9]) cases.gripRise.push({ id: `z${z}`, args: [z], out: gripRise(z) });

// ------------------------------------------------------------- rasterRest
{
  const line = (n) => Array.from({ length: n }, (_, i) => [i, 2 * i, 5 + i / 10]);
  for (const n of [0, 1, 2, 3, 5, 8, 12]) {
    for (const [a, b] of [[0, 0], [0, n], [1, n], [2, n - 1], [3, n - 3], [n, n], [4, 5], [0, 2]]) {
      if (b > n || a > b || cases.rasterRest.some((c) => c.id === `n${n}|${a}-${b}`)) continue;
      cases.rasterRest.push({ id: `n${n}|${a}-${b}`, args: [line(n), [a, b], cfgRef("PROP")], out: rasterRest(line(n), [a, b]).map((p) => ({ pts: Array.from(p), from: p.from })) });
    }
  }
}

// ------------------------------------------------------------- rasterTracks
for (const name of REGION_MODELS) {
  for (const deg of REGION_POSES) {
    for (const r of regionsOf(name, deg, 3)) {
      const tag = `${name}|r${deg.join(",")}|g${r.ri}`;
      const sup = (sway) => ({ topo: topoRef(name), rot: r.rot, offset: r.offset, sway });
      const real = (sway) => ({ topo: r.topo, rot: r.rot, offset: r.offset, sway });
      const part = seatedRef(name, r.rot, r.offset);
      for (const [vtag, step, span, sway] of [["d", 1, 12, false], ["sparse", 1, 30, false], ["dense", 1.5, 6, false], ["sway", 1, 12, true]]) {
        const lines = rasterTracks(r.pts, r.tris, r.soup, step, span, real(sway));
        cases.rasterTracks.push({ id: `${tag}|${vtag}`, args: [r.pts, r.tris, part, step, span, sup(sway), cfgRef("PROP")],
          out: { lines: Array.from(lines, (l) => Array.from(l)), spacing: lines.spacing } });
      }
    }
  }
}

// ------------------------------------------------------------- raceAgainst scenarios
function rng(seed) {
  let s = seed >>> 0;
  return () => ((s = Math.imul(s ^ (s >>> 15), 0x2c1b3c6d) + 0x9e3779b9 >>> 0) / 2 ** 32);
}
const TAGS = ["raster", "circular", "rim", "downhill", "curved", "flush"];
const RACE_TAGS = TAGS.slice(0, 5);   // the draws below pick from the old five: the flush tag is set after them

/** The walls of one pass, scripted around the real overhang faces. */
function mkPass(r, faces, nRegions, capLen, kind, opts) {
  const pick = (n) => Math.floor(r() * n);
  const rr = (a, b) => a + (b - a) * r();
  const triangles = [], props = [];
  let capUsed = 0;
  for (let ri = 0; ri < nRegions; ri++) {
    const own = faces.filter((f) => f[4] === ri);
    if (!own.length) continue;
    const n = pick(opts.maxPerRegion + 1);
    for (let k = 0; k < n; k++) {
      const f = own[pick(own.length)];
      const nSt = 2 + pick(5), horiz = rr(0, 6.28), len = rr(2, 14);
      const line = Array.from({ length: nSt }, (_, i) => [f[0] + Math.cos(horiz) * len * (i / nSt - 0.4) + rr(-0.5, 0.5),
        f[1] + Math.sin(horiz) * len * (i / nSt - 0.4) + rr(-0.5, 0.5), f[2] - 0.2 + rr(-1.2, 0.2)]);
      const t0 = triangles.length, nTri = 3 * (1 + pick(4));
      for (let i = 0; i < nTri; i++) triangles.push([line[i % nSt][0] + rr(-1, 1), line[i % nSt][1] + rr(-1, 1), rr(0, f[2]), r() < 0.2 ? 1 : 0]);
      const q = { region: ri, tines: pick(5), volume: rr(0.5, 30), height: rr(2, 30), area: rr(1, 50), line,
        triRanges: [[t0, triangles.length]], ...(r() < 0.25 ? { squat: true } : {}), ...(r() < 0.2 ? { partAttached: true } : {}),
        ...(r() < 0.6 ? { grip: rr(0.1, 25) } : r() < 0.5 ? { grip: Infinity } : {}) };
      if (capLen !== null) { const c0 = capUsed, c1 = c0 + pick(4); capUsed = c1; q.caps = [c0, c1]; }
      props.push(q);
    }
  }
  return { triangles, props, capUsed };
}

for (let seed = 1; seed <= 320; seed++) {
  const r = rng(seed * 104729);
  const pick = (n) => Math.floor(r() * n);
  const names = ["staircase", "lowledge", "portal", "torus", "arch", "lbracket", "ramp", "cone", "plus", "ushape", "tshape", "wedge"];
  const name = names[pick(names.length)];
  const deg = REGION_POSES[pick(REGION_POSES.length)];
  const p = posed(name, deg);
  const nR = p.res.regions.length;
  if (nR < 1) continue;
  // faces as overhangFaces gives them, to place the walls
  const faces = [];
  p.res.regions.forEach((g, ri) => {
    for (const f of g.faces) {
      let cx = 0, cy = 0, cz = 0;
      for (let i = 0; i < 3; i++) {
        const o = f * 9 + i * 3, x = p.topo.pos[o], y = p.topo.pos[o + 1], z = p.topo.pos[o + 2];
        cx += (p.rot[0] * x + p.rot[3] * y + p.rot[6] * z + p.res.offset.x) / 3;
        cy += (p.rot[1] * x + p.rot[4] * y + p.rot[7] * z + p.res.offset.y) / 3;
        cz += (p.rot[2] * x + p.rot[5] * y + p.rot[8] * z + p.res.offset.z) / 3;
      }
      faces.push([cx, cy, cz, p.topo.area[f], ri, f]);
    }
  });
  let tag = RACE_TAGS[pick(RACE_TAGS.length)];
  const capOn = r() < 0.5;
  const capLen = capOn ? 6 + pick(6) : null;
  const normal = mkPass(r, faces, nR, capLen, "n", { maxPerRegion: 2 });
  const second = mkPass(r, faces, nR, capLen, "s", { maxPerRegion: 3 });
  const scenario = {
    tag, maxGrow: r() < 0.3 ? 1.5 : null, rel: r() < 0.5 ? 0.1 : [0, 0.03, 0.5][pick(3)],
    lenient: r() < 0.2 ? (r() < 0.5 ? "always" : "evenLines") : null, equivalent: r() < 0.2,
    tried: r() < 0.15 ? Array.from({ length: nR }, (_, i) => i).filter(() => r() < 0.7) : null,
    ruler: r() < 0.2 ? "tight" : "default", price: r() < 0.2 ? 0.5 : null, veto: r() < 0.2 ? "small" : null,
    capFrom: capOn ? pick(capLen + 1) : 0, cap: capLen,
    normal: { triangles: normal.triangles, props: normal.props, served: pick(4), servedRegions: [0], tines: pick(10), sagRisk: r() < 0.2, volume: pick(100),
              skipped: { noLine: pick(3), wanders: 0, stub: 0, blocked: 0, degenerate: 0, buried: 0, weld: 0, sliver: pick(2), onPart: 0 } },
    second: { triangles: second.triangles, props: second.props, capLen: second.capUsed,
              sagRegions: r() < 0.3 ? [pick(nR)] : null,
              leaderOf: r() < 0.25 && nR > 1 ? Object.fromEntries(Array.from({ length: nR }, (_, i) => [i, pick(nR)]).map(([i, l]) => [i, l <= i ? l : i])) : null },
  };
  // the end-row race's two extras (prop/raster.js `alone`, `hold`), drawn last so the cases above stay as they were
  scenario.alone = r() < 0.2;
  scenario.hold = r() < 0.2 ? "bigWalls" : null;
  if (scenario.alone && r() < 0.6) scenario.tag = tag = "flush";
  cases.raceAgainst.push({ id: `seed${seed}|${name}|r${deg.join(",")}|${tag}`,
    args: [topoRef(name), analysisRef(name, 45, p.rot), p.rot, scenario, cfgRef("PROP")], out: runRace(p, scenario) });
}

export function runRace(p, s) {
  const { topo, res, rot } = p;
  const mk = (b, isNormal) => {
    const o = {
      triangles: b.triangles.map((v) => (v[3] ? [v[0], v[1], v[2], 1] : [v[0], v[1], v[2]])),
      props: b.props.map((q) => ({ ...q, line: q.line.map((v) => [...v]), triRanges: q.triRanges.map((t) => [...t]), ...(q.caps ? { caps: [...q.caps] } : {}) })),
      skipped: isNormal ? { ...b.skipped } : { noLine: 0, wanders: 0, stub: 0, blocked: 0, degenerate: 0, buried: 0, weld: 0, sliver: 0, onPart: 0 },
      served: b.served ?? 0, servedRegions: b.servedRegions ?? [], tines: b.tines ?? 0, sagRisk: b.sagRisk ?? false, volume: b.volume ?? 0,
    };
    if (b.sagRegions) o.sagRegions = new Set(b.sagRegions);
    if (b.leaderOf) o.leaderOf = new Map(Object.entries(b.leaderOf).map(([k, v]) => [+k, v]));
    return o;
  };
  const normal = mk(s.normal, true);
  const asked = [];
  const origCap = s.cap === null ? undefined : Array.from({ length: s.cap }, (_, i) => ({ x: i, y: 0, z: 0, biteX: 0, biteY: 0 }));
  globalThis.__TINECAP = origCap;
  const opts = s.veto === "small" ? { rasterVeto: (q) => q.volume < 5 } : {};
  const ruler = s.ruler === "tight" ? (fs, tops) => {
    let h = 0;
    for (const f of fs) if (tops.some((t) => Math.hypot(t[0] - f[0], t[1] - f[1]) <= 3 && f[2] - t[2] >= -0.05 && f[2] - t[2] <= 1.5)) h += f[3];
    return h;
  } : undefined;
  const lenient = s.lenient === "always" ? () => true : s.lenient === "evenLines" ? (qb) => qb.some((q) => q.line.length % 2 === 0) : null;
  const pass = (regions) => {
    asked.push([...regions]);
    if (globalThis.__TINECAP) for (let i = 0; i < s.second.capLen; i++) globalThis.__TINECAP.push({ x: 500 + i, y: 1, z: 0, biteX: 0, biteY: 0 });
    return mk(s.second, false);
  };
  let raced;
  try {
    raced = raceAgainst(topo, res, rot, opts, normal, s.capFrom, pass, s.tag, s.maxGrow, s.rel, lenient, s.equivalent,
      { tried: s.tried ? new Set(s.tried) : null, ruler, price: s.price, alone: !!s.alone,
        hold: s.hold === "bigWalls" ? (qa, qb, ri) => qa.some((q) => q.volume > 20) && qb.length > ri % 2 : undefined });
  } finally {
    const cap = globalThis.__TINECAP;
    globalThis.__TINECAP = undefined;
    var capOut = cap ? cap.map((c) => c.x) : null;
  }
  const props = raced.props.map((q) => ({ region: q.region, id: q.id, tines: q.tines ?? null, volume: q.volume ?? null, height: q.height ?? null,
    grip: q.grip ?? null, line: q.line, triRanges: q.triRanges, caps: q.caps ?? null, squat: !!q.squat, partAttached: !!q.partAttached,
    tags: TAGS.map((t) => (t in q ? q[t] : null)) }));
  return { triangles: raced.triangles, props, served: raced.served, servedRegions: raced.servedRegions, tines: raced.tines, sagRisk: raced.sagRisk,
    volume: raced.volume, swapped: raced[`${s.tag}Regions`] ?? null, sagRegions: raced.sagRegions ? [...raced.sagRegions] : null,
    asked, cap: capOut, sameAsNormal: raced === normal, skipped: raced.skipped };
}

for (const [fn, cs] of Object.entries(cases)) writeCases("raster", fn, cs);

// captured: the calls a real build makes over the 54 golden scenes (the capture cannot carry `spacing`:
// those cases hold the bare array of tracks; rasterRest's pieces carry `from`, which the capture drops too)
await recordGolden("raster", [{ rel: "prop/raster.js", fns: ["rasterTracks", "rasterRest"] }], {
  cap: 40,
  config: { rasterTracks: ["PROP"], rasterRest: ["PROP"] },
  configs: async (cap) => {
    const m = await import(new URL("web/prop/config.js", cap.rootUrl).href);
    return { PROP: () => m.PROP };
  },
});
