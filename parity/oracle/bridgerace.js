// Records parity/corpus/bridgerace/bridgeRace.jsonl (web/prop/bridgerace.js).
//
// bridgeRace returns a driver (begin / turned / finish) that buildProps runs from its patch loop, so each
// case is a SCENARIO: the loop is replayed here, with every patch's "build" a scripted push onto the
// accumulators (and the tests' tine capture, globalThis.__TINECAP, when `cap` is set).
//   args: [{ cap, patches, builds }]
//     cap      null (capture off) or the number of seeds already captured when the build starts
//     patches  [{ name, region, tris, turned?: [ux, uy] }] the queue; `turned` is the `lines.turned`
//              patchTracks gives that patch (every patch here takes the patch path)
//     builds   { [name]: { out: [[x,y,z]...], tines, sagRisk, skipped: {key: n}, served, caps: n seeds,
//                          wall: bool } }, a twin's under `<name>#twin`
//   out: { out, props: [{ region, tines, triRanges, wallRange?, caps? }], skipped, served: [...],
//          tally: { tines, sagRisk }, cap: seeds | null, patches: n }
// Replayed by crates/sf-engine/tests/parity_bridgerace.rs.
import { writeCases } from "./lib/encode.js";
import { assertPinned, WEB } from "./lib/pin.js";

assertPinned();
const { bridgeRace } = await import(`${WEB}prop/bridgerace.js`);

function rng(seed) {
  let s = seed >>> 0;
  return () => ((s = Math.imul(s ^ (s >>> 15), 0x2c1b3c6d) + 0x9e3779b9 >>> 0) / 2 ** 32);
}
const SKIP_KEYS = ["noLine", "wanders", "stub", "blocked", "degenerate", "buried", "weld", "sliver", "onPart"];

export function scenario(seed) {
  const r = rng(seed * 7919);
  const pick = (n) => Math.floor(r() * n);
  const rr = (a, b) => a + (b - a) * r();
  const cap = r() < 0.5 ? null : pick(4);
  const nP = 1 + pick(4);
  const patches = [], builds = {};
  const mkTris = () => {
    const cx = rr(-20, 20), cy = rr(-20, 20), cz = rr(6, 14), t = [];
    for (let f = 0; f < 1 + pick(3); f++) {
      const s = rr(2, 12);
      for (let k = 0; k < 3; k++) t.push(cx + rr(-s, s), cy + rr(-s, s), cz + rr(-0.3, 0.3));
    }
    return t;
  };
  const mkBuild = (tris) => {
    const n = pick(60), out = [];
    for (let i = 0; i < n; i++) {
      const b = pick(tris.length / 3) * 3;
      out.push([tris[b] + rr(-14, 14), tris[b + 1] + rr(-14, 14), tris[b + 2] + rr(-4, 1)]);
    }
    const skipped = {};
    for (const k of SKIP_KEYS) if (r() < 0.1) skipped[k] = 1 + pick(2);
    return { out: out.slice(0, n - (n % 3)), tines: pick(5), sagRisk: r() < 0.15, skipped, served: r() < 0.8,
             caps: cap === null ? 0 : pick(4), wall: r() < 0.4 };
  };
  for (let i = 0; i < nP; i++) {
    const tris = mkTris();
    const name = `p${i}`;
    patches.push({ name, region: pick(3), tris, ...(r() < 0.75 ? { turned: [Math.cos(rr(0, 3)), Math.sin(rr(0, 3))] } : {}),
                   // some patches carry the markers that veto a race
                   ...(r() < 0.1 ? { axis: [1, 0] } : {}) });
    builds[name] = mkBuild(tris);
    builds[`${name}#twin`] = mkBuild(tris);
    if (r() < 0.35) builds[`${name}#twin`].out = builds[name].out.map((v) => [v[0] + rr(-1, 1), v[1] + rr(-1, 1), v[2] + rr(-0.5, 0.5)]);
  }
  return { cap, patches, builds };
}

export function run(s) {
  const out = [], props = [], served = new Set(), tally = { tines: 0, sagRisk: false };
  const skipped = Object.fromEntries(SKIP_KEYS.map((k) => [k, 0]));
  globalThis.__TINECAP = s.cap === null ? undefined : Array.from({ length: s.cap }, (_, i) => ({ x: i, y: 0, z: 0, biteX: 0, biteY: 0 }));
  try {
    const race = bridgeRace({ out, props, skipped, served, tally });
    const patches = s.patches.map((p) => ({ ...p }));
    let seed = 0;
    for (let i = 0; i < patches.length; i++) {
      const patch = patches[i];
      race.begin(patch);
      race.turned(patch, patch.axis === undefined && patch.turned ? Object.assign([], { turned: patch.turned }) : Object.assign([], { turned: patch.turned }), patches);
      const b = s.builds[patch.bridgeTwin ? `${patch.name}#twin` : patch.name];
      const base = out.length, c0 = globalThis.__TINECAP ? globalThis.__TINECAP.length : null;
      for (const v of b.out) out.push([...v]);
      if (c0 !== null) for (let k = 0; k < b.caps; k++) globalThis.__TINECAP.push({ x: 1000 + seed++, y: 1, z: 2, biteX: 3, biteY: 4 });
      if (b.out.length) {
        props.push({ region: patch.region, tines: b.tines, triRanges: [[base, out.length]],
          ...(b.wall ? { wallRange: [base, base + Math.floor((out.length - base) / 2)] } : {}),
          ...(c0 === null ? {} : { caps: [c0, globalThis.__TINECAP.length] }) });
      }
      tally.tines += b.tines;
      tally.sagRisk ||= b.sagRisk;
      for (const k in b.skipped) skipped[k] += b.skipped[k];
      if (b.served) served.add(patch.region);
    }
    race.finish();
    return { out, props: props.map((q) => ({ region: q.region, tines: q.tines, triRanges: q.triRanges,
        ...(q.wallRange ? { wallRange: q.wallRange } : {}), ...(q.caps ? { caps: q.caps } : {}) })),
      skipped, served: [...served], tally, cap: globalThis.__TINECAP ? globalThis.__TINECAP.map((c) => c.x) : null, patches: patches.length };
  } finally {
    globalThis.__TINECAP = undefined;
  }
}

if (import.meta.main) {
  const cases = [];
  for (let seed = 1; seed <= 260; seed++) {
    const s = scenario(seed);
    cases.push({ id: `seed${seed}`, args: [{ ...s, patches: s.patches.map((p) => ({ ...p, tris: Float64Array.from(p.tris) })) }], out: run(s) });
  }
  writeCases("bridgerace", "bridgeRace", cases);
}
