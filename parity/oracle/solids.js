// Oracle recorder for web/solids.js -> parity/corpus/solids/<fn>.jsonl. Replayed in
// crates/sf-engine/tests/parity_solids.rs with Tol::Exact.
//
// Triangle lists are recorded as a flat Float64Array (x, y, z per vertex, three vertices per
// triangle): the exact bytes the JS vertex triples hold. A section is an array of [x, y, z].
// A polygon is an array of [a, b].
//
//   rounded_polygon  args: [poly, radius (number | array), steps]   out: polygon
//   rounded_flange   args: [sections]                               out: Float64Array
//   ribbon           args: [sections]                               out: Float64Array
//   box_extrude      args: [poly, lo, hi, frame]                    out: Float64Array
//   loft_extrude     args: [polyLo, polyHi, lo, hi, frame]          out: Float64Array
//   stack_extrude    args: [polys, zs, frame]                       out: Float64Array
// `frame` = {o, u, v, w}: P(a, b, c) = o + a*u + b*v + c*w, evaluated as
// ((o + a*u) + b*v) + c*w per component, a right-handed frame when u x v = w.
import { assertPinned, WEB } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();
const { roundedPolygon, roundedFlange, ribbon, boxExtrude, loftExtrude, stackExtrude } =
  await import(`${WEB}solids.js`);

function rng(seed) {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13; s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5; s >>>= 0;
    return s / 4294967296;
  };
}
const R = rng(0x51dc0de5);
const rnd = (lo, hi) => lo + R() * (hi - lo);
const flat = (tris) => Float64Array.from(tris.flat());

// --- polygons ------------------------------------------------------------------------------
/** A convex CCW polygon: n points on an ellipse, angles jittered but increasing. */
function convex(n, rx, ry, cx = 0, cy = 0, jitter = 0.25) {
  const pts = [];
  for (let i = 0; i < n; i++) {
    const a = ((i + (R() - 0.5) * jitter) / n) * 2 * Math.PI;
    pts.push([cx + rx * Math.cos(a), cy + ry * Math.sin(a)]);
  }
  return pts;
}
const rect = (x0, x1, y0, y1) => [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];

const polys = [
  rect(0, 1, 0, 1), rect(-2, 3, -1, 4), rect(0, 10, 0, 0.5), rect(0, 1e-6, 0, 1),
  [[0, 0], [1, 0], [0.5, 1]], [[0, 0], [1, 0], [1, 1e-9], [0, 0]],
  [[0, 0], [2, 0], [2, 0], [2, 1], [0, 1]], // a repeated corner
  [[0, 0], [1, 0], [2, 0], [2, 1], [0, 1]], // a straight-through corner (theta = pi)
  convex(6, 3, 3), convex(8, 5, 2), convex(5, 1, 4, 10, -3),
];
for (let i = 0; i < 40; i++) polys.push(convex(3 + Math.floor(R() * 10), rnd(0.5, 8), rnd(0.5, 8), rnd(-5, 5), rnd(-5, 5)));

// --- rounded_polygon ------------------------------------------------------------------------
{
  const cases = [];
  polys.forEach((poly, pi) => {
    const radii = [0.2, 0.5, 1, 2.5, 100, 0, -1, NaN];
    radii.forEach((r, ri) => {
      for (const steps of [8, 1, 3]) {
        if (steps !== 8 && ri % 2) continue;
        cases.push({ id: `p${pi}/r${ri}/s${steps}`, args: [poly, r, steps], out: roundedPolygon(poly, r, steps) });
      }
    });
    // per-corner radii, including a short array (undefined past its end)
    const per = poly.map((_, i) => (i % 2 ? 0 : rnd(0.1, 2)));
    cases.push({ id: `p${pi}/per`, args: [poly, per, 8], out: roundedPolygon(poly, per, 8) });
    cases.push({ id: `p${pi}/short`, args: [poly, per.slice(0, 2), 8], out: roundedPolygon(poly, per.slice(0, 2), 8) });
  });
  writeCases('solids', 'rounded_polygon', cases);
}

// --- frames, sections -----------------------------------------------------------------------
const frames = [
  { o: [0, 0, 0], u: [1, 0, 0], v: [0, 1, 0], w: [0, 0, 1] },
  { o: [1.5, -2, 0.25], u: [0, 1, 0], v: [0, 0, 1], w: [1, 0, 0] },
  { o: [0.1, 0.2, 0.3], u: [0.6, 0.8, 0], v: [-0.8, 0.6, 0], w: [0, 0, 1] },
];
const P = (f) => (a, b, c) => [0, 1, 2].map((k) => f.o[k] + a * f.u[k] + b * f.v[k] + c * f.w[k]);

/** A ring of a polygon lifted to height z in the XY plane. */
const ring = (poly, z) => poly.map(([x, y]) => [x, y, z]);

// --- box_extrude / loft_extrude / stack_extrude ---------------------------------------------
{
  const box = [], loft = [], stack = [];
  polys.forEach((poly, i) => {
    const f = frames[i % frames.length];
    for (const [lo, hi] of [[0, 1], [-0.5, 0.25], [2, 2], [0.3, 0.1]]) {
      const out = [];
      boxExtrude(poly, lo, hi, P(f), out);
      box.push({ id: `p${i}/${lo},${hi}`, args: [poly, lo, hi, f], out: flat(out) });
    }
    const hi = poly.map(([x, y]) => [x * rnd(0.5, 1.5) + rnd(-0.2, 0.2), y * rnd(0.5, 1.5)]);
    const out = [];
    loftExtrude(poly, hi, 0, 1.5, P(f), out);
    loft.push({ id: `p${i}`, args: [poly, hi, 0, 1.5, f], out: flat(out) });
    const layers = 2 + (i % 3);
    const stackPolys = [poly], zs = [0];
    for (let l = 1; l < layers; l++) {
      stackPolys.push(poly.map(([x, y]) => [x * (1 - 0.2 * l), y * (1 - 0.2 * l)]));
      zs.push(l * 0.7);
    }
    const so = [];
    stackExtrude(stackPolys, zs, P(f), so);
    stack.push({ id: `p${i}`, args: [stackPolys, zs, f], out: flat(so) });
  });
  writeCases('solids', 'box_extrude', box);
  writeCases('solids', 'loft_extrude', loft);
  writeCases('solids', 'stack_extrude', stack);
}

// --- ribbon ----------------------------------------------------------------------------------
const ribbons = [];
polys.forEach((poly, i) => {
  for (const n of [2, 3, 5]) {
    const secs = [];
    for (let l = 0; l < n; l++) secs.push(ring(poly.map(([x, y]) => [x * (1 + 0.1 * l), y]), l * 0.9));
    ribbons.push(secs);
  }
});
// k < 3 rings (no caps), and a single section
ribbons.push([[[0, 0, 0], [1, 0, 0]], [[0, 0, 1], [1, 0, 1]]]);
ribbons.push([[[0, 0, 0], [1, 0, 0], [0, 1, 0]]]);
{
  const cases = ribbons.map((secs, i) => {
    const out = [];
    ribbon(secs, out);
    return { id: `${i}`, args: [secs], out: flat(out) };
  });
  writeCases('solids', 'ribbon', cases);
}

// --- rounded_flange --------------------------------------------------------------------------
/** A flange section at point (x, y): 4 vertices, half-width `half`, floor zb, roof zt. */
const fsec = (x, y, sx, sy, half, zb, zt) => [
  [x + sx * half, y + sy * half, zb], [x + sx * half, y + sy * half, zt],
  [x - sx * half, y - sy * half, zt], [x - sx * half, y - sy * half, zb],
];
{
  const cases = [];
  const add = (id, secs) => {
    const out = [];
    roundedFlange(secs, out);
    cases.push({ id, args: [secs], out: flat(out) });
  };
  const line = (n, step, half = 3, angle = 0) => {
    const secs = [];
    const c = Math.cos(angle), s = Math.sin(angle);
    for (let i = 0; i < n; i++) secs.push(fsec(i * step * c, i * step * s, -s, c, half, 0, 0.6));
    return secs;
  };
  for (const n of [2, 3, 4, 6, 12]) for (const half of [1.5, 3]) add(`line${n}/${half}`, line(n, 2.5, half));
  add('line-angled', line(5, 3, 3, 0.7));
  // a bend: the collinear run collapses, the corner keeps a section
  const bent = [];
  for (let i = 0; i < 5; i++) bent.push(fsec(i * 2, 0, 0, 1, 3, 0, 0.6));
  for (let i = 1; i < 5; i++) bent.push(fsec(8, i * 2, 1, 0, 3, 0, 0.6));
  add('bent', bent);
  // zig-zag, so nothing collapses
  const zig = [];
  for (let i = 0; i < 6; i++) zig.push(fsec(i * 2, (i % 2) * 1.5, 0, 1, 2.5, 0, 0.6));
  add('zigzag', zig);
  // collinear but the roof dips at the middle: not on the line, kept
  const dip = line(3, 2.5);
  dip[1] = dip[1].map(([x, y, z]) => [x, y, z === 0.6 ? 0.5 : z]);
  add('dip', dip);
  // duplicated stations: len2 = 0 -> t = -1
  const dup = line(3, 2.5);
  dup[2] = dup[0].map((p) => [...p]);
  add('degenerate', dup);
  // random walks
  for (let i = 0; i < 40; i++) {
    const secs = [];
    let x = 0, y = 0, a = rnd(0, 6);
    const n = 2 + Math.floor(R() * 9);
    for (let j = 0; j < n; j++) {
      secs.push(fsec(x, y, -Math.sin(a), Math.cos(a), rnd(1, 4), rnd(0, 0.1), rnd(0.4, 1)));
      if (R() > 0.4) a += rnd(-0.6, 0.6);
      const step = rnd(0.5, 3);
      x += Math.cos(a) * step; y += Math.sin(a) * step;
    }
    add(`walk${i}`, secs);
  }
  // collinear runs with tiny noise (inside and outside the 1e-6 line test)
  for (const noise of [0, 1e-8, 5e-7, 2e-6, 1e-4]) {
    const secs = line(5, 2);
    secs[2] = secs[2].map(([x, y, z]) => [x, y + noise, z]);
    add(`noise${noise}`, secs);
  }
  writeCases('solids', 'rounded_flange', cases);
}
