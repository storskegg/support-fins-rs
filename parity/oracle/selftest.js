// Records parity/corpus/selftest/values.jsonl: one case per PARITY.md table row and per edge value.
// Each case is args: [value], out: value (the receiver-free identity case). No JS module is called.
import { assertPinned } from "./lib/pin.js";
import { writeCases } from "./lib/encode.js";

assertPinned();

// A Float32Array holding a NaN with a specific payload, to prove bytes survive.
const f32nan = new Float32Array(new Uint32Array([0x7fc00001, 0x3f800000]).buffer);

const values = {
  "num-finite": 1.5,
  "num-zero": 0,
  "num-neg-zero": -0,
  "num-nan": NaN,
  "num-inf": Infinity,
  "num-neg-inf": -Infinity,
  "num-min-subnormal": 5e-324,
  "num-max": 1.7976931348623157e308,
  "num-0.1+0.2": 0.1 + 0.2,
  "f64": new Float64Array([1, -0, NaN, Infinity, 5e-324, 0.1 + 0.2]),
  "f32": new Float32Array([1.5, -0, 0.1, Infinity]),
  "f32-nan": f32nan,
  "i32": new Int32Array([0, -1, 2147483647, -2147483648]),
  "u32": new Uint32Array([0, 1, 4294967295]),
  "u8": new Uint8Array([0, 1, 255]),
  "f64-empty": new Float64Array(0),
  "u8-empty": new Uint8Array(0),
  "str": "héllo   \"q\"",
  "bool-true": true,
  "bool-false": false,
  "null": null,
  "undefined": undefined,
  "array": [1, "a", null, undefined, -0, [NaN]],
  "array-empty": [],
  "object": { b: 1, a: [2], c: { d: null } },
  "object-empty": {},
  "object-dollar-key": { $n: 1, $$x: 2, "$": 3, a: 4 },
  "map": new Map([[1, "a"], ["k", [1, 2]], [-0, NaN]]),
  "set": new Set([3, 1, "x", -0]),
  "map-nested-in-object": { m: new Map([["k", new Map([[1, new Set([2])]])]]), z: 0 },
};

writeCases(
  "selftest",
  "values",
  Object.entries(values).map(([id, v]) => ({ id, args: [v], out: v })),
);
