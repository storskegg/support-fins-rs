// Records parity/corpus/attached/buildPartAttached.jsonl from web/prop/attached.js, called directly and as
// the 54 golden scenes' builds call it.
//   buildPartAttached  args: [line, partTris, $topo, rot, offset, out, state, {$config: {PROP, CUT, RESIN_FOOT}}]
//                      out: {ret, tris}
// `out` is recorded as [] (the call's `out` array starts empty for the replay; the function reads only what it
// pushes); `ret` is the verdict ({ok: true, prop} | {floored: why} | {}); `tris` a Float64Array of 4 numbers per
// vertex the call pushed (x, y, z, tag). `state` is the crest's module state {part, layerHeight} at the call
// (sweepBetween reads it): `part` a $seated reference, a full Float64Array, or null.
import { assertPinned, UPSTREAM_ROOT } from "./lib/pin.js";
import { captureModules, setScene, upstreamStatus } from "./lib/capture.js";
import { encode, writeCases } from "./lib/encode.js";
import { modelList } from "./lib/direct.js";

assertPinned();
const statusBefore = upstreamStatus();

const cap = await captureModules(
  [
    { rel: "overhangs.js", fns: ["buildTopology", "analyze"] },
    { rel: "fins/seating.js", fns: ["seatedPartTris"] },
    { rel: "prop/crest.js", fns: ["crestPart"] },
    { rel: "prop/attached.js", fns: ["buildPartAttached"] },
  ],
  {
    cap: 1e9,
    refs: true,
    quiet: ["buildTopology", "analyze", "seatedPartTris"],
    pushes: { buildPartAttached: 5 },
    copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"],
    args: {
      buildTopology: ([g]) => [g.getAttribute("position").array],
      analyze: ([, th, rot], sink) => [sink.model, th, rot],
    },
    config: { buildPartAttached: ["PROP", "CUT", "RESIN_FOOT"] },
    configs: new Proxy({}, { get: (_, k) => holder.thunks?.[k] }),
  },
);
const holder = {};
try {
  const m = cap.modules;
  const propMod = await import(`${cap.rootUrl}web/prop/config.js`);
  const cutMod = await import(`${cap.rootUrl}web/cutout.js`);
  const footMod = await import(`${cap.rootUrl}web/prop/resinfoot.js`);
  holder.thunks = { PROP: () => propMod.PROP, CUT: () => cutMod.CUT, RESIN_FOOT: () => footMod.RESIN_FOOT };
  const { contactLine } = await import(`${cap.rootUrl}web/prop/contact.js`);
  const { readSTL } = await import(`${cap.rootUrl}web/stl.js`);
  const { sitePose, run: runScene } = await import(`${cap.rootUrl}tests/_scene.js`);

  // --- direct: every model/pose, each region's contact line against the whole seated part ----------
  const POSES = [[0, 0, 0], [30, 0, 0], [40, 0, 0], [90, 0, 0], [135, 0, 0], [180, 0, 0], [0, 90, 0], [0, 180, 0], [45, 45, 0], [0, 35, 20], [-45, 30, 15], [60, 0, 90]];
  const variants = [["", {}], ["fg0-", { footGap: 0 }], ["loose-", { minStations: 2, minSpanPart: 1, minHeight: 0.8 }], ["thick-", { th: 2.4, tip: 1.2, sideClear: 0.4 }]];
  for (const [vn, vals] of variants) {
  const was = Object.fromEntries(Object.keys(vals).map((k) => [k, propMod.PROP[k]]));
  Object.assign(propMod.PROP, vals);
  for (const [name, path] of modelList()) {
    const pos = readSTL(Deno.readFileSync(path));
    for (const deg of POSES) {
      setScene(cap.sink, `direct-${vn}${name}-r${deg.join(",")}`);
      cap.sink.model = name;
      m["prop/crest.js"].crestPart(null);
      const topo = m["overhangs.js"].buildTopology({ getAttribute: () => ({ array: pos }) });
      const rot = sitePose(deg);
      const res = m["overhangs.js"].analyze(topo, 45, rot);
      const off = res.offset;
      const soup = m["fins/seating.js"].seatedPartTris(topo, rot, off);
      for (const region of res.regions.slice(0, 6)) {
        const tris = new Float64Array(region.faces.length * 9);
        const pts = [];
        region.faces.forEach((f, k) => {
          let gx = 0, gy = 0, gz = 0;
          for (let i = 0; i < 3; i++) {
            const o = f * 9 + i * 3, x = topo.pos[o], y = topo.pos[o + 1], z = topo.pos[o + 2];
            const sx = rot[0] * x + rot[3] * y + rot[6] * z + off.x, sy = rot[1] * x + rot[4] * y + rot[7] * z + off.y, sz = rot[2] * x + rot[5] * y + rot[8] * z + off.z;
            tris.set([sx, sy, sz], k * 9 + i * 3);
            pts.push([sx, sy, sz]);
            gx += sx; gy += sy; gz += sz;
          }
          pts.push([gx / 3, gy / 3, gz / 3]);
        });
        for (const ns of [12, 24, 40]) {
          const line = contactLine(pts, tris, ns);
          if (line) m["prop/attached.js"].buildPartAttached(line, soup, topo, rot, off, []);
        }
      }
    }
  }
  Object.assign(propMod.PROP, was);
  }

  // --- the golden scenes' own builds -------------------------------------------------------------
  const goldenDir = `${UPSTREAM_ROOT}/tests/golden`;
  const scenes = [...Deno.readDirSync(goldenDir)].filter((e) => e.name.endsWith(".json")).map((e) => e.name).sort()
    .map((n) => JSON.parse(Deno.readTextFileSync(`${goldenDir}/${n}`)).scene);
  for (const scene of scenes) {
    setScene(cap.sink, `build-${scene.name}`);
    cap.sink.model = scene.model;
    runScene(scene);
  }

  // --- assemble cases ----------------------------------------------------------------------------
  const cpLog = new Map(); // scene -> [crestPart call args (encoded, references substituted)]
  for (const e of cap.sink.calls.crestPart.entries()) {
    if (!cpLog.has(e.scene)) cpLog.set(e.scene, []);
    cpLog.get(e.scene)[e.n] = e.args;
  }
  const stateOf = (e) => {
    const a = e.cp > 0 ? cpLog.get(e.scene)?.[e.cp - 1] : null;
    return a ? { part: a[0], layerHeight: a[1] ?? { $u: 1 } } : { part: null, layerHeight: { $u: 1 } };
  };
  const all = cap.sink.calls.buildPartAttached.entries();
  const spread = (calls, n) => (calls.length <= n ? calls : Array.from({ length: n }, (_, i) => calls[Math.floor((i * calls.length) / n)]));
  const direct = all.filter((e) => e.scene.startsWith("direct-")), built = all.filter((e) => e.scene.startsWith("build-"));
  const chosen = [...spread(direct, 700), ...spread(built, 200)];
  // the verdicts are rare: keep every built wall and every refusal reason, thin the "no floor" bulk
  const kind = (e) => (e.out.ret.ok ? "ok" : e.out.ret.floored ? `floored-${e.out.ret.floored}` : "none");
  const seen = new Map();
  for (const e of all) seen.set(kind(e), (seen.get(kind(e)) ?? 0) + 1);
  const rare = all.filter((e) => kind(e) !== "none");
  const merged = [...new Map([...rare.slice(0, 600), ...chosen].map((e) => [`${e.scene}#${e.n}`, e])).values()]
    .sort((a, b) => all.indexOf(a) - all.indexOf(b));
  writeCases("attached", "buildPartAttached", merged.map((e) => ({
    id: `${e.scene}#${e.n}`,
    args: [...e.args.slice(0, 5), e.args[5], stateOf(e), e.args.at(-1)],
    out: e.out,
  })), { encoded: true });
  console.log("attached: calls", all.length, "recorded", merged.length, "verdicts", JSON.stringify(Object.fromEntries(seen)));
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
