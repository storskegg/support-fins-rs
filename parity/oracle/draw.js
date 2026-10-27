// Records parity/corpus/draw/{drawnLine,drawnWall}{,.captured}.jsonl[.zst] from web/draw.js, called directly
// and as the 54 golden scenes' builds (rows.js, fill.js) call it.
//   drawnLine  args: [a, b, $seated tris, step, band, under, through, {$config: {PROP}}]
//              out: [[x, y, z], ...] | null
//   drawnWall  args: [a, b, $seated tris, zBed, opts, {$config: {PROP, CUT, RESIN_FOOT, BRACE}}]
//              opts: { under, plateOnly, brace, braceTall, through, tines, tineDensity, layerHeight, topo, rot, offset }
//              (`topo` a $topo reference; `braceClear` is a function and is never passed)
//              out: { ok: false, reason } | { ok: true, tris (a Float64Array, 4 numbers per vertex: x, y, z, tag),
//                     length, height, partAttached?, squat?, tines, braces?, top, floors? }
// `drawnWall` sets the crest's module state itself (crestPart), so the replay needs none.
// Replayed by crates/sf-testkit/tests/parity_draw.rs.
import { assertPinned } from "./lib/pin.js";
import { Sampler, captureModules, flush, setScene, upstreamStatus } from "./lib/capture.js";
import { encode } from "./lib/encode.js";
import { goldenScenes } from "./lib/golden.js";
import { modelList } from "./lib/direct.js";

assertPinned();
const statusBefore = upstreamStatus();

const cap = await captureModules(
  [
    { rel: "overhangs.js", fns: ["buildTopology", "analyze"] },
    { rel: "fins/seating.js", fns: ["seatedPartTris"] },
    { rel: "draw.js", fns: ["drawnLine", "drawnWall"] },
  ],
  {
    cap: 40000,
    refs: true,
    quiet: ["buildTopology", "analyze", "seatedPartTris"],
    copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"],
    args: {
      buildTopology: ([g]) => [g.getAttribute("position").array],
      analyze: ([, th, rot], sink) => [sink.model, th, rot],
    },
    config: { drawnLine: ["PROP"], drawnWall: ["PROP", "CUT", "RESIN_FOOT", "BRACE"] },
    configs: new Proxy({}, { get: (_, k) => holder.thunks?.[k] }),
  },
);
const holder = {};
// drawnWall results carry whole wall soups: keep the direct cases and a thinner sample of the scenes'
cap.sink.calls.drawnWall = new Sampler(1700);
try {
  const m = cap.modules;
  const propMod = await import(`${cap.rootUrl}web/prop/config.js`);
  const cutMod = await import(`${cap.rootUrl}web/cutout.js`);
  const footMod = await import(`${cap.rootUrl}web/prop/resinfoot.js`);
  const braceMod = await import(`${cap.rootUrl}web/prop/brace.js`);
  holder.thunks = { PROP: () => propMod.PROP, CUT: () => cutMod.CUT, RESIN_FOOT: () => footMod.RESIN_FOOT, BRACE: () => braceMod.BRACE };
  const { readSTL } = await import(`${cap.rootUrl}web/stl.js`);
  const { sitePose, run: runScene } = await import(`${cap.rootUrl}tests/_scene.js`);

  // --- direct: each region's lines, several option sets ---------------------------------------------
  const POSES = [[40, 0, 0], [0, 35, 20], [90, 0, 0], [0, 0, 0]];
  const variants = [["", {}], ["fg0-", { footGap: 0 }], ["iface-", { iface: "flat", ifaceLayers: 2 }]];
  const MODELS = ["staircase", "lowledge", "portal", "torus", "arch", "lbracket", "ramp", "cone", "plus", "ushape", "tshape", "wedge", "cube", "sphere"];
  const models = modelList().filter(([name]) => MODELS.includes(name));
  const gate = new Map();
  for (const [vn, vals] of variants) {
    const was = Object.fromEntries(Object.keys(vals).map((k) => [k, propMod.PROP[k]]));
    Object.assign(propMod.PROP, vals);
    for (const [name, path] of models) {
      const pos = readSTL(Deno.readFileSync(path));
      for (const deg of vn === "" ? POSES : POSES.slice(0, 2)) {
        setScene(cap.sink, `direct-${vn}${name}-r${deg.join(",")}`);
        cap.sink.model = name;
        const topo = m["overhangs.js"].buildTopology({ getAttribute: () => ({ array: pos }) });
        const rot = sitePose(deg);
        const res = m["overhangs.js"].analyze(topo, 45, rot);
        const off = res.offset;
        const soup = m["fins/seating.js"].seatedPartTris(topo, rot, off);
        for (const region of res.regions.slice(0, 2)) {
          let x0 = Infinity, x1 = -Infinity, y0 = Infinity, y1 = -Infinity, zmin = Infinity, zsum = 0;
          for (const f of region.faces) {
            let gx = 0, gy = 0, gz = 0;
            for (let i = 0; i < 3; i++) {
              const o = f * 9 + i * 3, x = topo.pos[o], y = topo.pos[o + 1], z = topo.pos[o + 2];
              gx += rot[0] * x + rot[3] * y + rot[6] * z + off.x;
              gy += rot[1] * x + rot[4] * y + rot[7] * z + off.y;
              gz += rot[2] * x + rot[5] * y + rot[8] * z + off.z;
            }
            gx /= 3; gy /= 3; gz /= 3;
            x0 = Math.min(x0, gx); x1 = Math.max(x1, gx); y0 = Math.min(y0, gy); y1 = Math.max(y1, gy);
            zmin = Math.min(zmin, gz); zsum += gz;
          }
          const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2, zc = zsum / region.faces.length;
          const ex = Math.max(x1 - x0, 3), ey = Math.max(y1 - y0, 3);
          const L1 = [[cx - ex / 2, cy, zmin], [cx + ex / 2, cy, zmin]];
          const L2 = [[cx, cy - ey / 2, zmin], [cx, cy + ey / 2, zmin]];
          const L3 = [[cx - ex / 2, cy - ey / 2, zc], [cx + ex / 2, cy + ey / 2, zmin]];
          const L4 = [[cx - 0.4, cy, zmin], [cx + 0.4, cy, zmin]]; // too short
          const tinesOpts = { tines: true, tineDensity: undefined, layerHeight: 0.2, topo, rot, offset: off };
          const through = [cx, cy + ey / 3, zmin];
          const lite = vn !== "";
          const wall = (L, o) => m["draw.js"].drawnWall(L[0], L[1], soup, 0, o);
          const line = (L, ...r) => m["draw.js"].drawnLine(L[0], L[1], soup, ...r);
          for (const L of [L1, L2, L3]) {
            line(L, 1, Infinity, false, null);
            line(L, 1, 3, true, null);
          }
          line(L1, 1, Infinity, false, through);
          line(L4, 1, Infinity, false, null);
          line([L1[0], L1[0]], 1, Infinity, false, null); // a point: null
          wall(L1, {});
          wall(L3, {});
          if (lite) continue;
          wall(L1, { under: true });
          wall(L1, { plateOnly: true });
          wall(L1, tinesOpts);
          wall(L3, tinesOpts);
          wall(L2, { brace: true, braceTall: true });
          wall(L1, { through });
          wall(L1, { through, tines: true, topo, rot, offset: off, layerHeight: 0.2 });
          wall(L4, {});
          wall(L2, { under: true });
        }
      }
    }
    Object.assign(propMod.PROP, was);
  }

  // --- the golden scenes' own builds -------------------------------------------------------------
  for (const scene of goldenScenes()) {
    setScene(cap.sink, `build-${scene.name}`);
    cap.sink.model = scene.model;
    runScene(scene);
  }

  // Full coverage is where rows.js and fill.js call drawnWall / drawnLine by the thousand (the golden
  // scenes are Auto, which never does): a few Full builds, with their own tags
  for (const model of ["cube", "lbracket", "wedge", "staircase", "portal", "torus"]) {
    for (const rot of [[40, 0, 0], [0, 35, 20]]) {
      const scene = { name: `full-${model}-r${rot.join(",")}`, model, rot, set: { mode: "full" } };
      setScene(cap.sink, `build-${scene.name}`);
      cap.sink.model = scene.model;
      runScene(scene);
    }
  }

  // --- pack and write ----------------------------------------------------------------------------------
  // drawnWall's `tris` become one Float64Array of 4 numbers per vertex (x, y, z, tag)
  const walls = cap.sink.calls.drawnWall;
  const kinds = {};
  for (const e of walls.entries()) {
    const o = e.out;
    if (o && o.ok === true) {
      o.tris = encode(Float64Array.from(o.tris.flatMap((v) => [v[0], v[1], v[2], v.length > 3 && v[3] === 1 ? 1 : 0])));
    }
    const k = !o ? "throws" : !o.ok ? "refused" : o.squat ? "squat" : o.partAttached ? "partAttached" : "plate";
    kinds[k] = (kinds[k] ?? 0) + 1;
  }
  flush("draw", cap.sink, "run", ["drawnLine", "drawnWall"]);
  console.log("draw: drawnLine", cap.sink.calls.drawnLine.total, "drawnWall", walls.total, JSON.stringify(kinds));
} finally {
  cap.cleanup();
}
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
