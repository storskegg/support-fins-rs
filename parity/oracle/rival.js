// Records parity/corpus/rival/tubeRivalry.jsonl (web/prop/rival.js).
//
// tubeRivalry returns a driver (begin / tubeDone / finish) that buildProps runs from its patch loop,
// so each case is a SCENARIO: the loop is replayed here, with every patch's "build" a scripted push
// onto the accumulators.
//   args: [{ soup, regions, part, patches, builds }]
//     soup     Float64Array, 9 per face: the topology is buildTopology of it and the seated part (identity)
//     regions  [{ faces, area }] the analysis's regions
//     part     Float64Array, the seated part soup (= soup)
//     patches  [{ name, region, area, tris, reach?, arc?, smallTube?: [patch...] }]
//     builds   { [name]: { out: [[x,y,z]...], props: [{ region, tines }]?, tines, sagRisk, skipped: {key: n},
//                served } } what building that patch pushes
//   out: { out, props: [{ region, tines, triRanges }], skipped, served: [...], tally: { tines, sagRisk }, patches: n }
// Replayed by crates/sf-engine/tests/parity_rival.rs.
import { writeCases } from "./lib/encode.js";
import { assertPinned, WEB } from "./lib/pin.js";

assertPinned();

const { tubeRivalry } = await import(`${WEB}prop/rival.js`);
const { buildTopology } = await import(`${WEB}overhangs.js`);

function rng(seed) {
  let s = seed >>> 0;
  return () => ((s = Math.imul(s ^ (s >>> 15), 0x2c1b3c6d) + 0x9e3779b9 >>> 0) / 2 ** 32);
}

const SKIP_KEYS = ["noLine", "wanders", "stub", "blocked", "degenerate", "buried", "weld", "sliver", "onPart"];

export function scenario(seed) {
  const r = rng(seed);
  const pick = (n) => Math.floor(r() * n);
  const rr = (a, b) => a + (b - a) * r();
  const nFaces = 3 + pick(6);
  const soup = [];
  for (let f = 0; f < nFaces; f++) {
    const cx = rr(-25, 25), cy = rr(-25, 25), cz = rr(6, 14), s = rr(2, 9);
    for (let k = 0; k < 3; k++) soup.push(cx + rr(-s, s), cy + rr(-s, s), cz + rr(-0.4, 0.4));
  }
  const nReg = 1 + pick(Math.min(3, nFaces));
  const regions = Array.from({ length: nReg }, () => ({ faces: [], area: 0 }));
  for (let f = 0; f < nFaces; f++) regions[f < nReg ? f : pick(nReg)].faces.push(f);
  const regTris = (ri) => regions[ri].faces.flatMap((f) => soup.slice(f * 9, f * 9 + 9));
  const patches = [], builds = {};
  const vertsNear = (ri, n) => {
    const t = regTris(ri), out = [];
    for (let i = 0; i < n * 3; i++) {
      const b = pick(t.length / 3) * 3;
      out.push([t[b] + rr(-14, 14), t[b + 1] + rr(-14, 14), t[b + 2] + rr(-4, 1)]);
    }
    return out;
  };
  let uid = 0;
  const mkBuild = (ri) => {
    const nT = pick(4);
    const out = r() < 0.15 ? [] : vertsNear(ri, nT);
    const skipped = {};
    for (const k of SKIP_KEYS) if (r() < 0.12) skipped[k] = 1 + pick(2);
    return { out, props: out.length ? Array.from({ length: 1 + pick(2) }, () => ({ region: ri, tines: pick(3) })) : [],
             tines: pick(4), sagRisk: r() < 0.15, skipped, served: out.length > 0 && r() < 0.8 };
  };
  const mk = (ri, big) => {
    const name = `p${uid++}`;
    builds[name] = mkBuild(ri);
    return { name, region: ri, area: big ? rr(12, 60) : rr(1, 11.9), tris: regTris(ri), ...(r() < 0.15 ? { reach: regTris(ri).slice(0, 9) } : {}) };
  };
  const nTop = 1 + pick(4);
  for (let i = 0; i < nTop; i++) {
    const ri = pick(nReg);
    const p = mk(ri, true);
    if (r() < 0.65) {
      p.smallTube = Array.from({ length: pick(5) }, () => mk(ri, r() < 0.6));
      if (r() < 0.3) p.arc = true;
    }
    patches.push(p);
  }
  return { soup, regions, part: soup, patches, builds };
}

const clone = (p) => ({ ...p, ...(p.smallTube ? { smallTube: p.smallTube.map(clone) } : {}) });

export function run(s) {
  const topo = buildTopology({ getAttribute: (k) => (k === "position" ? { array: Float32Array.from(s.soup) } : null) });
  const result = { regions: s.regions };
  const out = [], props = [], served = new Set(), tally = { tines: 0, sagRisk: false };
  const skipped = Object.fromEntries(SKIP_KEYS.map((k) => [k, 0]));
  const rv = tubeRivalry({ out, props, skipped, served, tally }, topo, result, Float64Array.from(s.part));
  const patches = s.patches.map(clone);
  for (let i = 0; i < patches.length; i++) {
    const patch = patches[i];
    const mark = rv.begin(patch);
    const b = s.builds[patch.name];
    const base = out.length;
    for (const v of b.out) out.push([...v]);
    for (const q of b.props) props.push({ region: q.region, tines: q.tines, triRanges: [[base, out.length]] });
    tally.tines += b.tines;
    tally.sagRisk ||= b.sagRisk;
    for (const k in b.skipped) skipped[k] += b.skipped[k];
    if (b.served) served.add(patch.region);
    if (patch.smallTube) rv.tubeDone(patch, mark, patches);
  }
  rv.finish();
  return { out, props: props.map((q) => ({ region: q.region, tines: q.tines, triRanges: q.triRanges })), skipped, served: [...served], tally, patches: patches.length };
}

if (import.meta.main) {
  const cases = [];
  for (let seed = 1; seed <= 400; seed++) {
    const s = scenario(seed);
    cases.push({ id: `seed${seed}`, args: [{ ...s, soup: Float64Array.from(s.soup), part: Float64Array.from(s.part),
      patches: s.patches.map(function enc(p) { return { ...p, tris: Float64Array.from(p.tris), ...(p.reach ? { reach: Float64Array.from(p.reach) } : {}), ...(p.smallTube ? { smallTube: p.smallTube.map(enc) } : {}) }; }) }], out: run(s) });
  }
  writeCases("rival", "tubeRivalry", cases);
}
