# Parity: the oracle corpus

The JS engine at the `UPSTREAM` commit is the oracle. Deno scripts in `parity/oracle/` run
real JS functions and record `{args, out}` cases. Rust tests replay them through the
ported function and compare.

## Layout

```
parity/
  oracle/
    lib/encode.js          the encoder (this format) -- the only writer
    lib/pin.js             refuses to run unless the JS checkout is at UPSTREAM
    lib/capture.js         capture-by-rename: wrap a module's exports to record real calls
    <module>.js            one recorder per JS module, e.g. jsnum.js, three.js, overhangs.js
  corpus/
    <module>/<function>.jsonl[.zst]
  src/lib.rs               the decoder + replay helpers -- the only reader
```

`just oracle [module]` runs the recorders (all of them, or one) and rewrites their corpus
files. `just parity` runs the Rust replay tests. Corpus files are committed. Any file
over 5 MB is zstd-compressed (`.jsonl.zst`) and tracked with git LFS.

## Case format

One JSON object per line:

```json
{"id": "<stable unique id within the file>", "args": [<value>, ...], "out": <value>}
```

- `id` is deterministic (derived from the inputs, never a timestamp or random number), so
  re-recording an unchanged function gives an identical file.
- `args` holds the function's arguments in order. Recorders for methods put the receiver first,
  and the recorder's header comment says so.
- `out` is the return value. A function that throws records
  `{"id", "args", "throws": "<message>"}` instead of `out`.

## Value encoding

Plain JSON can't carry every JS number (`JSON.stringify(-0)` is `"0"`, and NaN and ±∞ become
`null`), so numbers and binary data are encoded:

| JS value | encoding |
|---|---|
| finite number, not -0 | JSON number, written by `JSON.stringify` (shortest round-trip) |
| -0, NaN, Infinity, -Infinity | `{"$n": "-0"}`, `{"$n": "NaN"}`, `{"$n": "Infinity"}`, `{"$n": "-Infinity"}` |
| `Float64Array` | `{"$f64": "<base64 of little-endian bytes>"}` |
| `Float32Array` | `{"$f32": "<base64 LE>"}` |
| `Int32Array` / `Uint32Array` / `Uint8Array` | `{"$i32": …}` / `{"$u32": …}` / `{"$u8": …}` |
| string, boolean, null | JSON as-is |
| `undefined` | `{"$u": 1}` |
| Array | JSON array of encoded values |
| Array with own enumerable non-index properties (a track's `spacing` / `turned`, a rim track's `rim`) | `{"$arr": [<encoded items>], "$props": {<encoded properties>}}`. The properties keep insertion order and a key starting with `$` is escaped as `$$`, as in a plain object. An array without such properties is a plain JSON array, so existing corpora are unchanged |
| plain object | JSON object of encoded values. Key order is preserved, and a key starting with `$` is escaped as `$$` |
| `Map` | `{"$map": [[k, v], ...]}` in insertion order |
| `Set` | `{"$set": [v, ...]}` in insertion order |

Typed arrays are encoded from their exact bytes, so a decoded `$f64` is bit-identical to what
JS held. Rust decodes with `serde_json`'s `float_roundtrip` feature on, so JSON numbers
round-trip exactly too.

## Comparing

`parity::compare(actual, expected, tol)` walks both values together and fails on the
**first** difference it finds, reporting the path (`out.fins[3].line[7][2]`), both values and the case id.

- Discrete values (strings, booleans, null, integers stored as integers, array lengths, object
  keys, Map and Set order) must be **equal**.
- Floats: `Tol::Exact` compares bits (all NaNs are equal; -0 ≠ +0). `Tol::Abs(t)`
  passes when `|a - e| <= t`, NaN only matches NaN, and ±∞ only matches itself.
- Default tolerances:
  - `jsnum`, `three`, file-format parsers: **`Exact`**
  - geometry outputs: **`Abs(1e-6)`** (mm). The ticket can set something tighter, never looser.

## Capture-by-rename

To record what a function actually sees inside a real build, `capture.js` loads a copy of the
module with `export function foo(` renamed to `function __foo(`, and appends
`export const foo = record('foo', __foo)`. The same technique is described in upstream
`docs/PERFORMANCE-SPEC.md`. The JS checkout is never modified: the copy is written to a temp
directory together with the rest of the `web/` tree. `record` encodes arguments *before* the call,
since JS functions sometimes mutate them. It caps each function's cases (the default is 200,
which the recorder can override) and picks them deterministically: the first N, then every Kth.

## Standard references in captured args (M1+)

A captured call's arguments often include the topology, an `analyze` result or a seated part
soup. Encoding those in full for every call would make corpora huge. They are always derivable
from the scene, so recorders replace them with references, and the Rust replay rebuilds them
through the ported functions. That way the replay also tests those functions.

| JS argument | recorded as | Rust replay resolves it with |
|---|---|---|
| a topology from `buildTopology` | `{"$topo": "<model id>"}` | `sf_testkit::parity::topo(id)`: `read_stl` + `build_topology`, cached per id |
| an `analyze` result | `{"$analysis": {"model": id, "threshold": t, "rot": [9 numbers]}}` | `sf_testkit::parity::analysis(..)`: `analyze(topo(id), t, rot)` |
| `seatedPartTris(topo, rot, offset)` | `{"$seated": {"model": id, "rot": [...], "offset": {x, y, z}}}` | `sf_testkit::parity::seated(..)` |
| a `Config` snapshot (the JS config objects at call time) | `{"$config": {"PROP": {...}, "FIN": {...}, ...}}`, only the objects the function reads | `sf_testkit::parity::config(..)`: `Config::default()`, then overwrite each listed field |

`<model id>` is the stress model's bare name (`cube`), or `fixtures/<name>` for `tests/fixtures`.
The recorder identifies the objects by identity: `capture.js` keeps a `WeakMap` from each
topology or result object it has seen to its reference, filled when `buildTopology`, `analyze` and
`seatedPartTris` return inside the captured tree. An argument that matches none of the
references is encoded in full.

Recorder API: `captureModules([{rel, fns}], {refs: true, config: {fn: ['PROP', ...]}, configs: {PROP: () => obj}})`.
A function listed in `config` gets **one extra trailing argument**, `{"$config": {...}}`, snapshotted at
call time. Its replay strips that argument and builds the `&Config` (the last parameter, per the type
mapping) with `sf_testkit::parity::config(..)`.

## Recording what a call leaves in `out`

Functions that push onto, compact or truncate a caller's `out` array (`sweep`, `dropCollapsed`,
`buildSquatBed`, …) are recorded by wrapping `out` in a `Proxy` for the call. The case records
`out.length` at entry (the absolute indices, such as `triRange`, depend on it) and the slice
`out[from..]` as it stands after the call, which covers in-place compaction and a rejected wall's
truncation. The replay seeds a Rust `out` of the same length, calls the function, and compares the same
slice. `capture.js`'s `opts.pushes` does this for captured calls.

## STEP import (`step`)

`parity/oracle/step.js` runs the web's own kernel (the vendored OpenCascade 7.6.1 WASM, occt-import-js 0.0.23) with
`STEP_PARAMS` on `tests/fixtures/*.step` and on 13 files that `parity/step-fixtures/gen.cpp` writes (spheres, cones, tori,
fillets, lofts, a revolved spline, a boolean pocket, loose shells and faces, a file in inches, a named nested assembly with
shared and moved instances, three free roots), plus five broken files. Three recorded functions: `isStep` (bytes, bool),
`readStepFile` (bytes; `success`, the assembly `root` tree and every mesh's `position` Float32Array and `index` Uint32Array,
as `web/stepworker.js` stores them) and `readStep` (bytes; `stepObjects` of that: name, soup, tris, meshes, bbox, or the
thrown message). `crates/sf-step/tests/parity_step.rs` replays them through the native kernel with **`Tol::Exact`**:
23 meshes, 85,430 vertices, 161,800 triangles, 0 differences (`cargo run -p sf-step --example parity_numbers`).

That exactness needs the same maths library, not only the same OpenCascade commit. Emscripten links musl's libm, and its
`sin`, `cos`, `atan2`, `pow` ... differ from Apple's in the last bit for a few percent of arguments (4.3% of a sphere's vertex
doubles, up to 1.8e-15). Rounded to f32 the vertices look identical, but the mesher decides on cocircular points by those
bits, and with Apple's libm 10 of the 23 meshes came out with other diagonals and another triangle order (the same vertices
and counts). `tools/occt/` therefore builds OpenCascade and the shim with every maths call renamed to musl's
(`wasm_math.h`, `libwasmmath.a` from Emscripten 3.1.69's sources), no fused multiply-add and no libm builtin rewriting.
