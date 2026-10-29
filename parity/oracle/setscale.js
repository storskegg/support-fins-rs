// Oracle recorder for the geometry of `setScale` (web/ui/scale.js:41-65) -> parity/corpus/setscale/setScale.jsonl.
//
// The page loads a part with setPart (web/ui/part.js:76-90): the bounding box of the file-frame Float32Array
// gives the centre (f64, `(min + max) / 2`), and `geometry.translate(-centre)` stores `f32(p - centre)` (THREE's
// BufferAttribute.applyMatrix4). That centred array is what scale.js keeps as `base.pos`. A scale is then
// `scaleAbout(base.pos, s, [0, 0, 0], centre)` = `f32(centre + s * f32(p - centre))`: two f32 roundings. (setPart
// then centres that result once more for the page; the Rust part stays in the file frame, so that last
// translation is not recorded.)
//
// A case: args [model id, percent text], out the scaled Float32Array. The model id is a stress model's bare name
// or `fixtures/<name>` (tests/fixtures), as sf_testkit::parity names them. The percent goes through the real
// `scaleFromPercent`. Replayed by crates/sf-ffi/tests/parity_setscale.rs with exact equality.
//
//   just oracle setscale
import { UPSTREAM_ROOT, WEB, assertPinned } from "./lib/pin.js";
import { writeCases } from "./lib/encode.js";

assertPinned();
const THREE = await import(`${WEB}vendor/three/three.core.js`);
const { readSTL } = await import(`${WEB}stl.js`);
const { scaleAbout, scaleFromPercent } = await import(`${WEB}scale.js`);

const stress = [...Deno.readDirSync(`${UPSTREAM_ROOT}/prototype/stress/models`)]
  .filter((e) => e.name.endsWith(".stl")).map((e) => e.name.slice(0, -4)).sort();
const fixtures = [...Deno.readDirSync(`${UPSTREAM_ROOT}/tests/fixtures`)]
  .filter((e) => e.name.endsWith(".stl")).map((e) => `fixtures/${e.name.slice(0, -4)}`).sort();
const percents = ["37.5", "100.01", "150", "250", "1000", "10", "33.333"];

/** setPart's centring, then setScale's `scaleAbout` of the centred base (web/ui/part.js:81-90, scale.js:44-52). */
function setScale(file, s) {
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(file.slice(), 3));
  geometry.computeBoundingBox();
  const bb = geometry.boundingBox;
  const centre = [(bb.min.x + bb.max.x) / 2, (bb.min.y + bb.max.y) / 2, (bb.min.z + bb.max.z) / 2];
  geometry.translate(-centre[0], -centre[1], -centre[2]);
  return scaleAbout(geometry.getAttribute("position").array.slice(), s, [0, 0, 0], centre);
}

const cases = [];
for (const model of [...stress, ...fixtures]) {
  const path = model.startsWith("fixtures/")
    ? `${UPSTREAM_ROOT}/tests/fixtures/${model.slice(9)}.stl`
    : `${UPSTREAM_ROOT}/prototype/stress/models/${model}.stl`;
  const file = readSTL(Deno.readFileSync(path));
  for (const pct of percents) {
    const s = scaleFromPercent(pct);
    cases.push({ id: `${model}@${pct}`, args: [model, pct], out: setScale(file, s) });
  }
}
writeCases("setscale", "setScale", cases);
console.log(`setscale: ${cases.length} cases`);
