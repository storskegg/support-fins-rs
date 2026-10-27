// Records parity/corpus/pad/buildPad.jsonl from web/fins/pad.js, called directly (`direct-*` scenes) and as
// the 54 golden scenes' builds call it (`build-*`).
//   buildPad  args: [contact (the lowest vertices, [x,y,z]...), partTris ($seated), out ([]), layerH, {$config: {PAD, FIN}}]
//             out: {ret, tris}: `ret` is null (fewer than three contact points) or the pad's info object
//             ({r1, r2, cells, height, points, oval, style, [autoSure,] smallFoot, outline}); `tris` a Float64Array
//             of 4 numbers per vertex the call pushed onto `out` (x, y, z, tag; a pad's vertices are untagged).
import { assertPinned } from "./lib/pin.js";
import { captureModules, setScene, upstreamStatus } from "./lib/capture.js";
import { writeCases } from "./lib/encode.js";
import { modelList } from "./lib/direct.js";

assertPinned();
const statusBefore = upstreamStatus();

const holder = {};
const cap = await captureModules(
  [
    { rel: "overhangs.js", fns: ["buildTopology", "analyze"] },
    { rel: "fins/seating.js", fns: ["seatedPartTris"] },
    { rel: "fins/pad.js", fns: ["buildPad"] },
  ],
  {
    cap: 1e9,
    refs: true,
    quiet: ["buildTopology", "analyze", "seatedPartTris"],
    pushes: { buildPad: 2 },
    copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"],
    args: {
      buildTopology: ([g]) => [g.getAttribute("position").array],
      analyze: ([, th, rot], sink) => [sink.model, th, rot],
    },
    config: { buildPad: ["PAD", "FIN"] },
    configs: new Proxy({}, { get: (_, k) => holder.thunks?.[k] }),
  },
);
try {
  const padMod = await import(`${cap.rootUrl}web/fins/pad.js`);
  const finMod = await import(`${cap.rootUrl}web/fins/config.js`);
  const { PAD } = padMod, { FIN } = finMod;
  holder.thunks = { PAD: () => PAD, FIN: () => FIN };
  const { buildTopology, analyze } = cap.modules["overhangs.js"];
  const { seatedPartTris, bedContact } = await import(`${cap.rootUrl}web/fins/seating.js`);
  const { readSTL } = await import(`${cap.rootUrl}web/stl.js`);
  const { sitePose, run: runScene } = await import(`${cap.rootUrl}tests/_scene.js`);
  const { buildPad } = cap.modules["fins/pad.js"];

  // A pad over a big footprint is megabytes of triangles: leave such calls out of the corpus (the
  // sampler is the capture's own, so take the call back off it).
  const LIMIT = 24000;
  const dropBig = (n) => {
    if (n > LIMIT) { const smp = cap.sink.calls.buildPad; smp.first.pop(); smp.total--; }
  };

  // --- direct: model x pose x pad style x layer height ---------------------------------------------
  const MODELS = new Set(["cube", "wedge", "lbracket", "torus", "sphere", "ramp", "cone", "tube", "portal", "fixtures/cube20", "arch"]);
  const POSES = [[0, 0, 0], [40, 0, 0], [0, 35, 20], [90, 0, 0], [45, 45, 0]];
  const STYLES = [
    ["auto", {}], ["light", {}], ["sure", {}], ["sure-petg", { grab: -0.1 }], ["auto-petg", { grab: -0.1 }],
    ["custom", { custom: { h: 0.5, gap: 0.1, grip: 0.05, margin: 3 } }],
    ["custom0", { custom: { h: 0.3, gap: 0, grip: -0.1, margin: 6 } }],
    ["light-bigsmall", { minGripOutline: 1e9 }],
  ];
  const LAYERS = [undefined, 0.3, 0];
  let kept = 0;
  for (const [name, path] of modelList()) {
    if (!MODELS.has(name)) continue;
    const pos = readSTL(Deno.readFileSync(path));
    for (const deg of POSES) {
      cap.sink.model = name;
      const topo = buildTopology({ getAttribute: () => ({ array: pos }) });
      const rot = sitePose(deg);
      const res = analyze(topo, 45, rot);
      const soup = seatedPartTris(topo, rot, res.offset);
      const contact = bedContact(topo, res, rot);
      for (const [stag, vals] of STYLES) {
        const was = structuredClone(PAD);
        Object.assign(PAD, vals);
        if (stag === "auto-petg") PAD.style = "auto";
        else if (stag === "sure-petg") PAD.style = "sure";
        else if (stag.startsWith("custom")) PAD.style = "custom";
        else if (stag === "light-bigsmall") PAD.style = "light";
        else PAD.style = stag;
        try {
          for (const layer of LAYERS) {
            if (layer !== undefined && stag !== "auto" && stag !== "light" && stag !== "custom") continue;
            setScene(cap.sink, `direct-${name}-r${deg.join(",")}-${stag}-l${layer}`);
            const o = [];
            buildPad(contact.pts, soup, o, layer);
            dropBig(o.length);
            kept++;
          }
        } finally {
          Object.assign(PAD, was);
        }
      }
    }
  }
  // fewer than three contact points
  setScene(cap.sink, "direct-few");
  {
    const name = "cube";
    const pos = readSTL(Deno.readFileSync(modelList().find(([n]) => n === name)[1]));
    cap.sink.model = name;
    const topo = buildTopology({ getAttribute: () => ({ array: pos }) });
    const rot = sitePose([0, 0, 0]);
    const res = analyze(topo, 45, rot);
    buildPad([[0, 0, 0], [1, 1, 0]], seatedPartTris(topo, rot, res.offset), [], 0.2);
  }

  // --- the golden scenes' own builds ----------------------------------------------------------------
  const { goldenScenes } = await import("./lib/golden.js");
  for (const scene of goldenScenes()) {
    setScene(cap.sink, `build-${scene.name}`);
    cap.sink.model = scene.model;
    runScene(scene);
  }
  // (the golden scenes' pads are kept whole: they are what buildFins really builds)

  const all = cap.sink.calls.buildPad.entries().filter((e) => (e.out.tris.$f64.length * 3) / 4 / 32 <= LIMIT);
  const built = all.filter((e) => e.scene.startsWith("build-"));
  console.log("b64 chars", all.reduce((a, e) => a + e.out.tris.$f64.length, 0));
  writeCases("pad", "buildPad", all.map((e) => ({ id: `${e.scene}#${e.n}`, args: e.args, out: e.out })), { encoded: true });
  const kinds = {};
  for (const e of all) { const s = e.out.ret?.style ?? "null"; kinds[s] = (kinds[s] ?? 0) + 1; }
  console.log("pad: calls", all.length, "direct", all.length - built.length, "built", built.length, JSON.stringify(kinds));
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
