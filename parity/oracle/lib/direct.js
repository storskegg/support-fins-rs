// Shared by the module recorders' direct cases: the models, poses and reference makers.
import { assertPinned, UPSTREAM_ROOT, WEB } from "./pin.js";
import { encode, Ref } from "./encode.js";

assertPinned();

const { buildTopology, analyze } = await import(`${WEB}overhangs.js`);
const { readSTL } = await import(`${WEB}stl.js`);
const { seatedPartTris } = await import(`${WEB}fins/seating.js`);
const { sitePose } = await import(`${WEB}../tests/_scene.js`);

export { analyze, buildTopology, seatedPartTris, sitePose };

/** [name, path] for the stress models (bare name) and tests/fixtures STLs (`fixtures/<name>`). */
export function modelList() {
  const out = [];
  for (const [dir, prefix] of [[`${UPSTREAM_ROOT}/prototype/stress/models`, ""], [`${UPSTREAM_ROOT}/tests/fixtures`, "fixtures/"]]) {
    const names = [...Deno.readDirSync(dir)].filter((e) => e.isFile && e.name.endsWith(".stl")).map((e) => e.name).sort();
    for (const n of names) out.push([prefix + n.replace(/\.stl$/, ""), `${dir}/${n}`]);
  }
  return out;
}

const topoCache = new Map();
/** The topology of a model id, built once. */
export function topoOf(name) {
  if (!topoCache.has(name)) {
    const path = modelList().find(([n]) => n === name)[1];
    const pos = readSTL(Deno.readFileSync(path));
    topoCache.set(name, buildTopology({ getAttribute: (k) => (k === "position" ? { array: pos } : null) }));
  }
  return topoCache.get(name);
}

/** Poses used by the direct cases (sitePose degrees). */
export const POSES = [[0, 0, 0], [40, 0, 0], [0, 35, 20], [90, 0, 0], [-45, 30, 15]];

/** analyze + seat a model in a pose: {topo, rot, res, offset, soup}. */
export function posed(name, deg, threshold = 45) {
  const topo = topoOf(name);
  const rot = sitePose(deg);
  const res = analyze(topo, threshold, rot);
  return { topo, rot, res, offset: res.offset, soup: seatedPartTris(topo, rot, res.offset) };
}

export const topoRef = (name) => new Ref({ $topo: name });
export const analysisRef = (name, threshold, rot) =>
  new Ref({ $analysis: { model: name, threshold: encode(threshold), rot: encode(Array.from(rot)) } });
export const seatedRef = (name, rot, offset) =>
  new Ref({ $seated: { model: name, rot: encode(Array.from(rot)), offset: encode(offset) } });

/** Fractions of a bounding box used to place query columns (a little outside, edges, inside). */
export const FRACS = [-0.05, 0.02, 0.2, 0.37, 0.5, 0.63, 0.81, 0.98, 1.05];

/** XY extent of a soup. */
export function extentXY(tris) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (let i = 0; i < tris.length; i += 3) {
    if (tris[i] < x0) x0 = tris[i]; if (tris[i] > x1) x1 = tris[i];
    if (tris[i + 1] < y0) y0 = tris[i + 1]; if (tris[i + 1] > y1) y1 = tris[i + 1];
  }
  return { x0, y0, x1, y1 };
}

// --- config snapshots for direct cases ---
const cfgMods = {
  PROP: (await import(`${WEB}prop/config.js`)).PROP,
  FIN: (await import(`${WEB}fins/config.js`)).FIN,
  TRACE: (await import(`${WEB}prop/trace.js`)).TRACE,
  JUDGE: (await import(`${WEB}prop/judge.js`)).JUDGE,
  DOWNHILL: (await import(`${WEB}prop/downhill.js`)).DOWNHILL,
  FIELD: (await import(`${WEB}fins/field.js`)).FIELD,
};
export { cfgMods };

/** A trailing {"$config": {NAME: ...}} snapshot of the live config objects, taken now. */
export const cfgRef = (...names) => new Ref({ $config: Object.fromEntries(names.map((n) => [n, encode(cfgMods[n])])) });

/** Run `fn` with config fields overridden, then restore them. */
export function withConfig(over, fn) {
  const saved = {};
  for (const [name, fields] of Object.entries(over)) {
    saved[name] = {};
    for (const [k, v] of Object.entries(fields)) { saved[name][k] = cfgMods[name][k]; cfgMods[name][k] = v; }
  }
  try { return fn(); } finally {
    for (const [name, fields] of Object.entries(saved)) for (const [k, v] of Object.entries(fields)) cfgMods[name][k] = v;
  }
}

/**
 * The biggest regions of a posed model as buildProps sees them: seated faces (`tris`), and the
 * vertices + centroids (`pts`). At most `max` regions.
 */
export function regionsOf(name, deg, max = 3) {
  const p = posed(name, deg);
  const { topo, rot, res, offset } = p;
  const out = [];
  for (const [ri, region] of res.regions.slice(0, max).entries()) {
    const tris = new Float64Array(region.faces.length * 9);
    const pts = [];
    region.faces.forEach((f, k) => {
      let gx = 0, gy = 0, gz = 0;
      for (let i = 0; i < 3; i++) {
        const o = f * 9 + i * 3, x = topo.pos[o], y = topo.pos[o + 1], z = topo.pos[o + 2];
        const sx = rot[0] * x + rot[3] * y + rot[6] * z + offset.x;
        const sy = rot[1] * x + rot[4] * y + rot[7] * z + offset.y;
        const sz = rot[2] * x + rot[5] * y + rot[8] * z + offset.z;
        tris[k * 9 + i * 3] = sx; tris[k * 9 + i * 3 + 1] = sy; tris[k * 9 + i * 3 + 2] = sz;
        pts.push([sx, sy, sz]);
        gx += sx; gy += sy; gz += sz;
      }
      pts.push([gx / 3, gy / 3, gz / 3]);
    });
    out.push({ ri, region, tris, pts, ...p });
  }
  return out;
}

/** Models that have overhang regions at a pose, for the region-based recorders (a small, varied set). */
export const REGION_MODELS = ["cube", "wedge", "lbracket", "torus", "sphere", "ramp", "arch", "cone", "tube", "portal", "lowledge", "staircase"];
export const REGION_POSES = [[40, 0, 0], [0, 35, 20], [90, 0, 0], [0, 0, 0]];
