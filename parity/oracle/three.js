// Records the three.js r185 rotation routines `sf_engine::three` ports, and the two
// pose builders on top of them (tests/_scene.js sitePose, plugins/cli/cli.js rotationMatrix).
// Replayed by crates/sf-engine/tests/parity_three.rs with Tol::Exact.
//
// Case shapes (args -> out):
//   from_axis_angle         [[ax, ay, az], angle]      -> {x, y, z, w}
//   multiply_quaternions    [a, b]                      -> {x, y, z, w}   (a * b)
//   premultiply             [self, q]                   -> {x, y, z, w}   (receiver first; q * self)
//   matrix4_from_quaternion [q]                         -> 16 numbers (Matrix4.elements)
//   matrix4_from_euler      [x, y, z, "XYZ"]            -> 16 numbers
//   matrix3_from_matrix4    [16 numbers]                -> 9 numbers (Matrix3.elements)
//   deg_to_rad              [degrees]                   -> radians
//   site_pose               [[dx, dy, dz], eps]         -> 9 numbers
//   cli_rotation_matrix     [[dx, dy, dz]]              -> 9 numbers
// A quaternion is {x, y, z, w}. Every number is encoded by parity/oracle/lib/encode.js, so
// -0 and the rest survive.

import { assertPinned, UPSTREAM_ROOT, WEB } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();

const THREE = await import(`${WEB}vendor/three/three.core.js`);
// Both import cleanly under the recorder's permissions, so the real functions are called.
const { sitePose } = await import(`file://${UPSTREAM_ROOT}/tests/_scene.js`);
const { rotationMatrix } = await import(`file://${UPSTREAM_ROOT}/plugins/cli/cli.js`);

const N = 2000;
const PI = Math.PI;

// --- deterministic inputs -------------------------------------------------------------

/** xorshift32 in [0, 1), fixed seed per stream. */
function rng(seed) {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13; s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5; s >>>= 0;
    return s / 4294967296;
  };
}

const num = (x) => (Object.is(x, -0) ? '-0' : String(x));
const quatObj = (q) => ({ x: q.x, y: q.y, z: q.z, w: q.w });
const AXES = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
const axisVec = (a) => new THREE.Vector3(a[0], a[1], a[2]);

/** Drops cases whose id was already used (the lists below overlap on purpose). */
function unique(cases) {
  const seen = new Set();
  return cases.filter((c) => !seen.has(c.id) && seen.add(c.id));
}

/** `n` angles: every multiple of 5 deg in [-360, 360] via degToRad, 0, -0, +-1e-9, then xorshift in +-4pi. */
function angles(n, seed) {
  const out = [];
  for (let d = -360; d <= 360; d += 5) out.push(THREE.MathUtils.degToRad(d));
  out.push(0, -0, 1e-9, -1e-9);
  const r = rng(seed);
  const seen = new Set(out.map(num));
  while (out.length < n) {
    const a = (r() * 2 - 1) * 4 * PI;
    if (!seen.has(num(a))) { seen.add(num(a)); out.push(a); }
  }
  return out;
}

const quatFrom = (axisIdx, angle) =>
  new THREE.Quaternion().setFromAxisAngle(axisVec(AXES[axisIdx]), angle);

/** n deterministic quaternions, each a setFromAxisAngle about a world axis (the first n of a stream). */
function quats(n, seed) {
  const r = rng(seed);
  const out = [];
  for (let i = 0; i < n; i++) out.push(quatFrom(Math.floor(r() * 3), (r() * 2 - 1) * 4 * PI));
  return out;
}

/** A quaternion with all three axes mixed in, so matrices are not axis-aligned. */
function mixedQuats(n, seed) {
  const r = rng(seed);
  const out = [];
  for (let i = 0; i < n; i++) {
    const q = quatFrom(0, (r() * 2 - 1) * 4 * PI);
    q.premultiply(quatFrom(1, (r() * 2 - 1) * 4 * PI));
    q.premultiply(quatFrom(2, (r() * 2 - 1) * 4 * PI));
    out.push(q);
  }
  return out;
}

// --- from_axis_angle ------------------------------------------------------------------

{
  const cases = [];
  for (let ai = 0; ai < 3; ai++) {
    for (const a of angles(N, 0x1234abcd + ai)) {
      cases.push({
        id: `axis${ai}/${num(a)}`,
        args: [AXES[ai], a],
        out: quatObj(new THREE.Quaternion().setFromAxisAngle(axisVec(AXES[ai]), a)),
      });
    }
  }
  writeCases('three', 'from_axis_angle', unique(cases));
}

// --- multiply_quaternions / premultiply -----------------------------------------------

{
  const as = quats(N, 0x2468ace1), bs = quats(N, 0x13579bdf);
  // fixed leading cases: identity, and the identity against a turn
  as.unshift(new THREE.Quaternion());
  bs.unshift(new THREE.Quaternion());
  const mul = [], pre = [];
  for (let i = 0; i < N; i++) {
    const a = as[i], b = bs[i];
    const id = `${i}`;
    mul.push({
      id, args: [quatObj(a), quatObj(b)],
      out: quatObj(new THREE.Quaternion().multiplyQuaternions(a, b)),
    });
    // receiver first: self.premultiply(q) is q * self
    pre.push({
      id, args: [quatObj(a), quatObj(b)],
      out: quatObj(a.clone().premultiply(b)),
    });
  }
  writeCases('three', 'multiply_quaternions', mul);
  writeCases('three', 'premultiply', pre);
}

// --- matrix4_from_quaternion / matrix3_from_matrix4 -----------------------------------

{
  // the 4 exact 90-degree turn families first, then axis turns, then mixed ones
  const qs = [new THREE.Quaternion()];
  for (let ai = 0; ai < 3; ai++) {
    for (let k = -4; k <= 4; k++) qs.push(quatFrom(ai, THREE.MathUtils.degToRad(90 * k)));
  }
  qs.push(...quats(600, 0x0badf00d), ...mixedQuats(N, 0x7a3c91e5));
  qs.length = N;
  const cases = qs.map((q, i) => ({
    id: `${i}`, args: [quatObj(q)],
    out: [...new THREE.Matrix4().makeRotationFromQuaternion(q).elements],
  }));
  writeCases('three', 'matrix4_from_quaternion', cases);

  // matrix3_from_matrix4: general (non-rotation) 4x4s as well as rotations, so every one of the
  // nine entries is distinguishable from its neighbours.
  const r = rng(0x5eed1234);
  const m3 = [];
  for (let i = 0; i < N; i++) {
    const elements = i < 1000
      ? [...new THREE.Matrix4().makeRotationFromQuaternion(mixedQuats(1, 0x100 + i)[0]).elements]
      : Array.from({ length: 16 }, () => (r() * 2 - 1) * 10);
    const m4 = new THREE.Matrix4();
    m4.elements.splice(0, 16, ...elements);
    m3.push({ id: `${i}`, args: [elements], out: [...new THREE.Matrix3().setFromMatrix4(m4).elements] });
  }
  writeCases('three', 'matrix3_from_matrix4', m3);
}

// --- matrix4_from_euler (XYZ) ---------------------------------------------------------

{
  const r = rng(0x600dcafe);
  const cases = [];
  const push = (x, y, z) => {
    const e = new THREE.Euler(x, y, z, 'XYZ');
    cases.push({
      id: `${cases.length}`, args: [x, y, z, 'XYZ'],
      out: [...new THREE.Matrix4().makeRotationFromEuler(e).elements],
    });
  };
  push(0, 0, 0);
  push(-0, -0, -0);
  for (const d of [-180, -90, 0, 90, 180]) {
    push(THREE.MathUtils.degToRad(d), 0, 0);
    push(0, THREE.MathUtils.degToRad(d), 0);
    push(0, 0, THREE.MathUtils.degToRad(d));
  }
  while (cases.length < N) push(...[0, 0, 0].map(() => (r() * 2 - 1) * 2 * PI));
  writeCases('three', 'matrix4_from_euler', cases);
}

// --- deg_to_rad -----------------------------------------------------------------------

{
  const cases = [];
  const seen = new Set();
  const push = (d) => {
    if (seen.has(num(d))) return;
    seen.add(num(d));
    cases.push({ id: num(d), args: [d], out: THREE.MathUtils.degToRad(d) });
  };
  for (let d = -720; d <= 720; d++) push(d);
  const r = rng(0x0d15ea5e);
  let n = 0;
  while (n < 1000) { const before = cases.length; push((r() * 2 - 1) * 1000); n += cases.length - before; }
  writeCases('three', 'deg_to_rad', cases);
}

// --- site_pose / cli_rotation_matrix --------------------------------------------------

{
  const poses = [];
  const seen = new Set();
  const addPose = (p) => {
    const k = p.map(num).join(',');
    if (!seen.has(k)) { seen.add(k); poses.push(p); }
  };
  const steps45 = [];
  for (let d = -180; d <= 180; d += 45) steps45.push(d);
  for (const x of steps45) for (const y of steps45) for (const z of steps45) addPose([x, y, z]);
  for (let ai = 0; ai < 3; ai++) {
    for (let d = -180; d <= 180; d += 5) {
      const p = [0, 0, 0];
      p[ai] = d;
      addPose(p);
    }
  }
  // the poses of all 54 golden scenes
  const dir = `${UPSTREAM_ROOT}/tests/golden`;
  const files = [...Deno.readDirSync(dir)].map((e) => e.name).filter((n) => n.endsWith('.json')).sort();
  if (files.length !== 54) throw new Error(`expected 54 golden scenes, found ${files.length}`);
  for (const f of files) addPose(JSON.parse(Deno.readTextFileSync(`${dir}/${f}`)).scene.rot);

  const site = [];
  for (const p of poses) {
    for (const eps of [0, 1e-9, -1e-9]) {
      site.push({ id: `${p.map(num).join(',')}/${num(eps)}`, args: [p, eps], out: [...sitePose(p, eps)] });
    }
  }
  writeCases('three', 'site_pose', site);

  writeCases('three', 'cli_rotation_matrix', poses.map((p) => ({
    id: p.map(num).join(','), args: [p], out: [...rotationMatrix(p)],
  })));
}
