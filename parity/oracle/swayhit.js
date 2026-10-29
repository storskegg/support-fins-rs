// Records parity/corpus/swayhit/{lowestHit,cropForRib}.jsonl from web/swayhit.js and web/swaycrop.js (direct calls on
// the stress models; the topology argument is {"$topo": id}, the part's triangles are topo.pos[0 .. nFaces*9]).
//   lowestHit   args: [topo, fr, uLo, uHi, [z0, z1, s0, slope, depth], sLo, sHi, zLo, zHi]
//               fr: {nh: {x, y}, uDir: {x, y}}. The outline is the four edges tests/triindex.test.js builds from the
//               parameters: z >= z0, z <= z1, s >= s0 + slope*z - 0.05, s <= s0 + slope*z + depth + 0.1.
//               The recorded value is the indexed call with the z bounds; the oracle also checks the full scan agrees.
//               out: number (Infinity when nothing crosses)
//   cropForRib  args: [topo, o, r, lean, zBed, zTop]   out: Float64Array (the kept triangles)
import { assertPinned, UPSTREAM_ROOT, WEB } from "./lib/pin.js";
import { Ref, writeCases } from "./lib/encode.js";
import { upstreamStatus } from "./lib/capture.js";

assertPinned();
const statusBefore = upstreamStatus();

const { buildTopology } = await import(`${WEB}overhangs.js`);
const { lowestHit, lowestHitBrute } = await import(`${WEB}swayhit.js`);
const { cropForRib } = await import(`${WEB}swaycrop.js`);
const { readSTL } = await import(`${WEB}stl.js`);
const geometry = (pos) => ({ getAttribute: (k) => (k === "position" ? { array: pos } : null) });

function rng(seed) {
  return () => {
    seed = (seed * 1664525 + 1013904223) >>> 0;
    return seed / 2 ** 32;
  };
}

const dir = `${UPSTREAM_ROOT}/prototype/stress/models`;
const names = [...Deno.readDirSync(dir)].filter((e) => e.isFile && e.name.endsWith(".stl") && Deno.statSync(`${dir}/${e.name}`).size < 1e6)
  .map((e) => e.name.replace(/\.stl$/, "")).sort();

const hits = [], crops = [];
let nHit = 0, nMiss = 0, nCrop = 0;
for (const name of names) {
  const pos = readSTL(Deno.readFileSync(`${dir}/${name}.stl`));
  const topo = buildTopology(geometry(pos));
  const tris = Float64Array.from(topo.pos.subarray(0, topo.nFaces * 9));
  const ref = new Ref({ $topo: name });
  const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < tris.length; i += 3) for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], tris[i + k]); hi[k] = Math.max(hi[k], tris[i + k]); }
  const rand = rng(name.length * 7919 + 1);
  const pad = [(hi[0] - lo[0]) * 0.2, (hi[1] - lo[1]) * 0.2, 0];
  const L = lo.map((v, k) => v - pad[k]), H = hi.map((v, k) => v + pad[k]);
  for (let n = 0; n < 60; n++) {
    const ang = rand() * Math.PI * 2;
    const nh = { x: Math.cos(ang), y: Math.sin(ang) };
    const uDir = { x: -nh.y, y: nh.x };
    const cx = L[0] + rand() * (H[0] - L[0]), cy = L[1] + rand() * (H[1] - L[1]);
    const uc = cx * uDir.x + cy * uDir.y, s0 = cx * nh.x + cy * nh.y;
    const th = 1 + rand() * 4, depth = 3 + rand() * 25, slope = (rand() - 0.5) * 0.4;
    const z0 = rand() * (H[2] - L[2]) * 0.5, z1 = z0 + 2 + rand() * (H[2] - L[2]);
    const sIn = (z) => s0 + slope * z, sOut = (z) => s0 + slope * z + depth;
    const outline = [
      (q) => q[2] - z0, (q) => z1 - q[2],
      (q) => q[0] - (sIn(q[2]) - 0.05), (q) => (sOut(q[2]) + 0.1) - q[0],
    ];
    const sLo = Math.min(sIn(z0), sIn(z1)) - 0.05, sHi = Math.max(sOut(z0), sOut(z1)) + 0.1;
    const uLo = uc - th / 2 - 0.3, uHi = uc + th / 2 + 0.3;
    const got = lowestHit(tris, { nh, uDir }, uLo, uHi, outline, sLo, sHi, z0, z1);
    const want = lowestHitBrute(tris, { nh, uDir }, uLo, uHi, outline);
    if (!Object.is(got, want)) throw new Error(`${name} ${n}: lowestHit ${got} vs scan ${want}`);
    if (got === Infinity) nMiss++; else nHit++;
    hits.push({ id: `${name}|q${n}`, args: [ref, { nh, uDir }, uLo, uHi, [z0, z1, s0, slope, depth], sLo, sHi, z0, z1], out: got });
  }
  for (let n = 0; n < 40; n++) {
    const o = [lo[0] - 25 + rand() * (hi[0] - lo[0] + 50), lo[1] - 25 + rand() * (hi[1] - lo[1] + 50)];
    const lean = [(rand() - 0.5) * 0.8, (rand() - 0.5) * 0.8];
    const r = 2 + rand() * 20, zBed = n % 4 === 0 ? 0 : rand() * 3, zTop = n % 3 === 0 ? Infinity : 10 + rand() * 60;
    const out = cropForRib(tris, o, r, lean, zBed, zTop);
    nCrop += out.length > 0 ? 1 : 0;
    crops.push({ id: `${name}|c${n}`, args: [ref, o, r, lean, zBed, zTop], out });
  }
}
if (nHit < 100 || nMiss < 100 || nCrop < 50) throw new Error(`corpus too thin: ${nHit} hits ${nMiss} misses ${nCrop} non-empty crops`);
writeCases("swayhit", "lowestHit", hits);
writeCases("swayhit", "cropForRib", crops);
console.log(`swayhit: ${hits.length} lowestHit (${nHit} hit, ${nMiss} miss), ${crops.length} cropForRib (${nCrop} non-empty), ${names.length} models`);
if (upstreamStatus() !== statusBefore) throw new Error("the upstream checkout changed during the run");
