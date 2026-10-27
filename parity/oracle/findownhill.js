// Records parity/corpus/findownhill/*.jsonl from web/fins/downhill.js: `heldArea` on real builds (direct) and as
// `judgeDownhill` calls it in the 54 golden scenes (`heldArea.captured`). `usedDownhill` is an arrow constant (the capture
// cannot wrap it) and `judgeDownhill` takes a closure, so they are tested by their own Rust tests and through buildFins.
//   heldArea  args: [$topo, {over, offset}, rot, {fins: [{line}]}, cls (Uint8Array), {PROP}]    out: number
//             ({over, offset} is all heldArea reads of the analyze result; `built` is trimmed to the fin lines)
import { assertPinned, WEB } from "./lib/pin.js";
import { captureModules, setScene, upstreamStatus } from "./lib/capture.js";
import { encode, Ref, writeCases } from "./lib/encode.js";
import { analyze, cfgMods, modelList, sitePose, topoOf, topoRef } from "./lib/direct.js";
import { goldenScenes } from "./lib/golden.js";

assertPinned();
const statusBefore = upstreamStatus();
const PROP = cfgMods.PROP;
const down = await import(`${WEB}fins/downhill.js`);
const { classify } = await import(`${WEB}fins/coverage.js`);
const finsMod = await import(`${WEB}fins.js`);
const cfg = () => new Ref({ $config: { PROP: encode(PROP) } });
const slimBuilt = (b) => ({ fins: (b.fins ?? []).map((f) => ({ line: f.line })) });
const slimResult = (r) => ({ over: Uint8Array.from(r.over), offset: { ...r.offset } }); // analyze returns scratch arrays: copy

const direct = [];
for (const [name] of modelList()) {
  const topo = topoOf(name);
  for (const deg of [[0, 0, 0], [40, 0, 0], [90, 0, 0], [0, 35, 20]]) {
    const rot = sitePose(deg);
    const res = analyze(topo, 45, rot);
    const cls = classify(topo, res, rot);
    for (const mode of ["prop", "auto"]) {
      const built = finsMod.buildFins(topo, res, rot, { mode, bedPad: true, tines: true, layerHeight: 0.2, tallRibs: false });
      for (const span of [undefined, 3]) {
        const was = PROP.maxUnsupportedSpan;
        if (span !== undefined) PROP.maxUnsupportedSpan = span;
        try {
          const args = [topoRef(name), slimResult(res), rot, slimBuilt(built), cls, cfg()];
          direct.push({ id: `${name}|r${deg.join(",")}|${mode}|s${span}`, args, out: down.heldArea(topo, res, rot, built, cls) });
        } finally {
          PROP.maxUnsupportedSpan = was;
        }
      }
    }
  }
}
writeCases("findownhill", "heldArea", direct);

const holder = {};
const cap = await captureModules(
  [
    { rel: "overhangs.js", fns: ["buildTopology", "analyze"] },
    { rel: "fins/downhill.js", fns: ["heldArea"] },
  ],
  {
    cap: 1e9, refs: true, quiet: ["buildTopology", "analyze"],
    copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"],
    args: {
      buildTopology: ([g]) => [g.getAttribute("position").array],
      analyze: ([, th, rot], sink) => [sink.model, th, rot],
      heldArea: ([topo, result, rot, built, cls]) => [topo, slimResult(result), rot, slimBuilt(built), cls],
    },
    config: { heldArea: ["PROP"] },
    configs: new Proxy({}, { get: (_, k) => holder.thunks?.[k] }),
  },
);
try {
  const propMod = await import(`${cap.rootUrl}web/prop/config.js`);
  holder.thunks = { PROP: () => propMod.PROP };
  const { run: runScene } = await import(`${cap.rootUrl}tests/_scene.js`);
  for (const scene of goldenScenes()) {
    setScene(cap.sink, scene.name);
    cap.sink.model = scene.model;
    runScene(scene);
  }
  const entries = cap.sink.calls.heldArea.entries();
  writeCases("findownhill", "heldArea.captured", entries.map((e) => ({ id: `${e.scene}#${e.n}`, args: e.args, out: e.out })), { encoded: true });
  console.log("findownhill: direct", direct.length, "captured", entries.length);
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
