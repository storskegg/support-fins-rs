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
