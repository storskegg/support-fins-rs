# Swift ↔ Rust interface (`crates/sf-ffi`)

The Rust engine runs **inside the app process**. `sf-ffi` is a `staticlib` with a UniFFI 0.32
interface (proc-macros, no UDL). `just swift-bindings` builds it for `aarch64-apple-darwin` and
generates the Swift package at `app/SupportFinsCore/`: an XCFramework plus the UniFFI Swift
sources and a thin hand-written Swift layer. The app depends on that package.

## Principles
- **Zero copy for geometry.** Meshes and fin triangles cross as `GpuBuffer` objects. These are
  Rust-owned, page-aligned (16 KiB) allocations of `f32` vertex data. Swift wraps one with
  `MTLDevice.makeBuffer(bytesNoCopy:length:options:.storageModeShared, deallocator:)`, and
  the deallocator releases the Rust object. Unified memory means the CPU and GPU read the same pages.
  UniFFI can't carry a raw pointer, so `GpuBuffer::address()` returns a `u64`. The Swift layer turns it into an
  `UnsafeMutableRawPointer`, and that conversion is the only unsafe Swift in the app.
- **The engine stays exact.** The FFI layer only converts formats and schedules work. It adds no geometry
  logic, and every build goes through `sf_engine::fins::build::build_fins` exactly as the CLI does.
- **Off the main thread.** Builds run as jobs on a Rust worker thread. Results come back through a
  UniFFI callback interface on that thread, and the Swift layer hops to the main actor.
- **Cancellation: by generation number, and the build in flight stops (M5).** Each new build for a part
  bumps a counter and raises the cancel flag of the build in flight; the engine polls that flag
  (`sf_engine::cancel`, carried in the build's `Config`) between passes, regions, walls and stations and
  inside its Rayon loops, so a stale build stops part-way (a few milliseconds on a typical part, under
  ~20 ms on a 772k-triangle object: `just bench-cancel`) and a cancelled build returns a distinct result,
  never a partial one. A finished job whose generation is stale is still dropped without being
  delivered. `Builder.cancel()` abandons the build in flight without starting another. An uncancelled
  build is byte-identical to one that was never cancellable.
- **Input-taking exports throw (X-11, C-06).** A Rust panic in a non-throwing export reaches the generated Swift as a
  `try!`, which traps and kills the app. So **every export that takes data derived from a file or the user** (JSON, text,
  numbers, coordinates, indices, vectors, paths, bytes, or an object built from them) returns `Result<_, FfiError>`, and
  its body runs under `crash::guarded(operation, ..)` (`crates/sf-ffi/src/crash.rs`): the operation name is set for the
  crash report, and a panic becomes `FfiError::Engine("internal error: <message>")` (the panic hook of the crash dumps
  still writes its report first). The app handles the throw where it already shows failures (the status line, a model's
  `error`, a failed ranking) and uses `try?` only where a default is right, with the reason beside it. These stay
  non-throwing: crash-report plumbing that must never fail, and the pure getters of an object already built (listed
  with the reason for each in `GETTERS` in `crates/sf-ffi/tests/hostile.rs`, which also scans `src/` so a new export
  that is neither throwing nor listed fails the test). A new export is `Result` unless it is one of those.
  Numeric inputs are range-checked at the door (a rotation's entries within 2, a scale 0..1000, a plate coordinate
  within 4000 mm, a gap within 1000 cells, a wide-search density 0..100, print-space points within 100000 mm): a value
  far past anything real would otherwise allocate without bound, and an allocation failure aborts instead of unwinding.
  `sf_io::json::parse` refuses deep nesting (`limits::MAX_JSON_DEPTH`; a hostile 3MF session could otherwise overflow the stack).
- **Listener callbacks never kill a worker.** `BuildListener` / `SuggestListener` are called from the `sf-build` /
  `sf-suggest` threads inside `crash::listener_call`, which catches a panic (UniFFI panics when the Swift side reports an
  unexpected error from a callback), leaves its report to the crash hook, and lets the worker serve the next job.
  `Builder::build` / `Suggester::*` return an error instead of dropping the job if the worker is gone.
- **Errors** are a UniFFI error enum (`FfiError::{Io, Format, Engine, Cancelled, LimitExceeded}`) carrying the JS-identical
  message where one exists. `LimitExceeded` is the one a host answers rather than shows: see "Load limits".

## Surface (M4a; extended in M4b)

```rust
#[uniffi::export] fn engine_version() -> String;            // crate version + UPSTREAM commit

#[derive(uniffi::Object)] struct GpuBuffer;                  // Rust-owned, page-aligned
impl GpuBuffer { fn address(&self) -> u64; fn len_bytes(&self) -> u64; fn stride(&self) -> u32; fn count(&self) -> u32; }

#[derive(uniffi::Record)] struct PartInfo { name: String, triangles: u64, size: [f64; 3] /* as Vec<f64> */, notes: Vec<String> /* import notes, ui/io.js */, import_note: String /* renderImportNote: "3MF: a; b." / "OBJ: ..." / "STEP: ..." / empty */, weld_ms: f64 }
#[derive(uniffi::Object)] struct Part;                       // Arc<Topology> + source soup
fn load_parts(path: String) -> Result<Vec<Arc<Part>>, FfiError>;  // STL / OBJ / 3MF / STEP (one Part per object, negative parts cut out as ui/io.js does). STEP (.step/.stp, or any file whose content is "ISO-10303-21;") goes through sf-step: OpenCascade 7.6.1 at 0.01 mm / 0.1 rad as web/step.js, the assembly tree picks the objects (stepObjects), names come from the file, import_note is "STEP: [imported “X” of N objects; ]tessellated at 0.01 mm." A build without OpenCascade fails the load with "STEP import is not available in this build ...": the app's own build (sf-step's `occt` feature off, X-01; the app reads STEP with the two functions below)
fn reads_as_step(path: String) -> Result<bool, FfiError>;  // X-01: whether load_parts would read the file as STEP (not named .stl/.obj/.3mf; named .step/.stp, or its first 64 bytes start "ISO-10303-21;"). Io error when it cannot be read
fn load_step_parts(path: String, kernel: Vec<u8>) -> Result<Vec<Arc<Part>>, FfiError>;  // X-01: the parts of a STEP file from the StepReader XPC service's read (`kernel`: sf_step::wire bytes), the parts load_parts makes of it in process; the bytes are decoded as untrusted (Format "the STEP reader sent back a malformed result"; a tree with an index out of range or a cycle is "the CAD kernel could not read this STEP file")
fn load_parts_with(path: String, options: LoadOptions) -> Result<Vec<Arc<Part>>, FfiError>;  // load_parts with the load options ("Load limits" below); load_parts is load_parts_with(path, LoadOptions { allow_large: false })
fn load_step_parts_with(path: String, kernel: Vec<u8>, options: LoadOptions) -> Result<Vec<Arc<Part>>, FfiError>;  // load_step_parts with the load options: the STEP's parts get the same limits as load_parts_with (finite coordinates; a part over 2 m is LimitExceeded unless allow_large)
fn merge_parts(parts: Vec<Arc<Part>>, object_count: u64) -> Result<Arc<Part>, FfiError>;  // the picker's "Merge N & load": positions concatenated in the order given (io.js mergeObjectPositions), welded as one soup; colours merged by triangle, slicer parts kept; named after the file; note "merged N of TOTAL objects into one part" (TOTAL = object_count, the file's); one part returns itself
fn scale_from_percent_text(text: String) -> Result<Option<f64>, FfiError>;  // M6: scaleFromPercent(Number(text)): 10-1000 % clamped, rounded to the field's 2 decimals; None for junk, empty or <= 0
fn scale_percent_text(factor: f64) -> Result<String, FfiError>;             // M6: the field's text, +(factor * 100).toFixed(2) as JS prints it ("150", "33.33")
impl Part {
  fn info(&self) -> PartInfo;
  fn mesh(&self) -> Arc<GpuBuffer>;                          // interleaved position+normal, f32, model frame
  fn analyze(&self, threshold_deg: f64, rot: Vec<f64>) -> Result<AnalysisSummary, FfiError>;   // rot: 9, column-major
  fn face_flags(&self, threshold_deg: f64, rot: Vec<f64>) -> Result<Arc<GpuBuffer>, FfiError>; // u8 per face: over/kept/on_bed bits
  fn scaled(&self, factor: f64) -> Result<Arc<Part>, FfiError>;                // M6 Scale (ui/scale.js setScale): uniform scale about the box centre with the page's two f32 roundings (centred `f32(p - c)`, then `f32(c + s * centred)`; exact vs the oracle `parity/oracle/setscale.js`), welded afresh; slicer parts scale with it, colours/notes/name kept. Call it on the part as loaded (never on a scaled one: no compounding)
}
#[derive(uniffi::Record)] struct AnalysisSummary { over_area: f64, bed_area: f64, over_face_count: u64, region_count: u64, offset: Vec<f64>, size: Vec<f64> }

// M4b:
#[uniffi::export(callback_interface)] trait BuildListener: Send + Sync { fn done(&self, generation: u64, result: Arc<BuildResult>); fn failed(&self, generation: u64, err: FfiError); }
#[derive(uniffi::Object)] struct Builder;                    // one per part; owns the generation counter + worker
impl Builder { fn new(part: Arc<Part>) -> Arc<Self>; fn build(&self, rot: Vec<f64>, opts_json: String, tunables_json: String, listener: Box<dyn BuildListener>) -> Result<u64, FfiError>; fn cancel(&self); }
#[derive(uniffi::Object)] struct BuildResult;                // FinsBuilt + the seated part
impl BuildResult { fn fins(&self) -> Arc<GpuBuffer>; fn pad(&self) -> Arc<GpuBuffer>; fn summary_json(&self) -> String; fn records_json(&self) -> String; }
fn export(path: String, parts: Vec<Arc<Part>>, builds: Vec<Arc<BuildResult>>, options_json: String) -> Result<(), FfiError>; // STL/3MF as the CLI writes them
```

`opts_json` and `options_json` take exactly the CLI's / plugin options (`options.json` keys), parsed
by the same code the CLI uses. The page's settings form and the CLI flags therefore can't drift apart.

### M4b as implemented

- **`opts_json`** is a JSON object of `options.json` keys (`{}` = the engine defaults). It goes through
  `optionsFromDialog` and `computeFins` (`sf_cli::entry::compute_fins_detail`), the CLI's own path, with no
  second parser. The `rot` is the pose as nine column-major numbers; the identity pose is left unposed, as the
  CLI leaves a file with no `--rot`. A bad option or pose arrives through `failed` as `FfiError::Engine`.
- **`tunables_json`** is the page's `tunables` object, `finOpts().tunables` (`web/ui/finbuild.js:139`): `roundFeet`,
  `padH`, `padGrab`, `padStyle`, `padCustom` (`{ h, gap, grip, margin }`), `propGap`, `tallRibsHeight`, `cutout`,
  `iface`, `tineDir`, `pinDepth`, `nozzle`, `wallLines` (and `tineContactOffset`, `tinesPerWall`). `{}` or empty text
  means none. A key present replaces the value the dialog options would have given; a key absent keeps it.
  **Ranges (S-07).** Every entry that takes `tunables_json` (`Builder.build`, `build_part*`, the suggest entries,
  `DrawKit::new`, `build_report`) refuses a numeric value outside what the inspector's controls clamp to, with
  `FfiError::Engine` ("tunable nozzle is 0, outside 0.1 to 2"): `propGap` 0.1 to 0.4, `tallRibsHeight` 1 to 2000 mm (the
  field is 5 to 200 cm; the lower bound is looser because the parity corpus builds with 8 mm), `pinDepth` 0.05 to 1,
  `nozzle` 0.1 to 2, `wallLines` one of 2, 4, 6, 8, `padCustom` `h` 0.1 to 1, `gap` 0 to 0.5, `grip` -0.2 to 0.3,
  `margin` 1 to 12, `tineContactOffset` 0 to 10, `tinesPerWall` 0 to 1000. The JS engine ignores such values (a junk
  nozzle or wall-line count is skipped, the rest taken as given); the app never sends them, so this changes no build
  it makes. A key that is absent or not a number is left to the engine as before.
- **The request-level extras.** Four settings of the page's build request are neither `options.json` keys nor
  tunables, so they ride in `opts_json` beside the dialog keys: `tallRibs` (bool, "Brace tall walls"), `curvedWalls`
  (bool), `sway.uncappedDepth` (bool, "full brace depth") and `sway.crossRise` (fraction 0..1, the cross-rib height).
  With sway on, a host that sends `tallRibs` also gets the page's `crossAbove` (the tall-wall height when ticked, else
  never) and the tunables' `roundFeet` for the braces, as `swayOpts()` sets them.
- **Which build runs.** With no `tunables_json` and none of the extras, a build is the CLI's (`computeFins`: the
  posed part is re-seated, and `export` of it is byte-identical to the CLI's file). With either, it is **the page's
  build**: `buildFins` on the part's own topology in this pose with the `finOpts()` request, the tunables applied inside
  `buildFins` as `applyTunables`, exactly as `web/finworker.js` runs it. `parity/oracle/ffibuild.js` records such builds
  from the JS Worker path and `crates/sf-ffi/tests/web_build.rs` checks the FFI build against them (hash of the
  triangles, counts, every record).
- **Generations** start at 1. A job that is no longer the latest when it starts or when it finishes is dropped:
  `done`/`failed` are only called for the newest generation, on the builder's worker thread.
- **`BuildResult.fins()` / `pad()`** are `GpuBuffer`s of the same interleaved layout as `Part.mesh()` (position
  xyz + the triangle's unit normal xyz, `f32`, stride 24, three vertices per triangle) in print space, the
  seated frame the page draws. `fins()` holds walls, wedges and sway braces; `pad()` the bed pad.
- **`summary_json()`**: `{ stats, report, offset, mode, volume, tines, braceCount, propCount, unserved,
  sagRisk, smallHoleRegions, patchCount, patchStats, skipped, rejected, servedRegions, bare, floating, sway,
  tallRibs }`. `stats` and `report` are the CLI's `--json` stats and one-line report; **`patchStats` and
  `skipped` carry only the counters that counted anything** (JS leaves the others absent, #23).
- **`records_json()`**: `{ fins: [...], props: [...], crestRanges: [[from, to), ...] }`. `crestRanges` are the vertex ranges
  (into the `fins()` buffer) of the interface crest, the runs of consecutive tagged triangles the page draws in the
  second material (`ui/scene.js` `meshFrom`; a triangle is crest when its first vertex is tagged, `splitInterface`):
  they are empty unless the interface material (`iface`) is on. A `fins` entry is `{ id, kind (prop|wedge|sway), height,
  length, tines, rows, stilt, lean, bearing, span, short, fill, joined, braces?, ribs?, triRanges, line }`;
  `triRanges` are `[from, to)` **vertex** ranges into the `fins()` buffer (what per-fin removal cuts out). A
  `props` entry is `{ id, region?, tines, span, height, fill, squat, rimWall, inBore? }`: **a fill-pass wall has no
  `region`** (and its `id` is a placeholder: `fill` says so) (`PropRecord.region` is an `Option`, #23).
- **`export`** writes **what the page exports** (`web/ui/export.js`), not what the CLI's `finFile` writes: the part as
  oriented and seated on the plate, with the fins and the pad in the same file. `path`'s extension picks STL or 3MF.
  `parts` and `builds` hold one part and its build (several is an `Engine` error until the app has a plate layout).
  `options_json` (`{}` is the defaults; an unknown key is an error):
  - `finsOnly: true` (STL): just the fins and the pad, **in the part's print position whatever the pose** (the CLI's
    refuse-when-posed rule does not apply);
  - `fins: false`: the part alone, seated in the pose (STL or 3MF). Give the `pose` (nine numbers, column-major) and
    `threshold` (default 45); `builds` may then be empty, the seating coming from an analysis of the pose;
  - `removed: [ids]`: fins removed by hand (`records_json` fin ids) are left out, the pad stays (`filteredTriangles`);
  - 3MF only: `bed: [x, y]` (the build volume; the part is written where it stands on a centred bed) and
    `placed: [x, y]` (the model's spot on the plate, `liveXY()`). An STL ignores `placed`, as the page does.
  The bytes are the page's (`parity/oracle/ffiexport.js` writes them through the real writers, `crates/sf-ffi/tests/export.rs`
  compares): STL header `{name} - support-fins` with the file's name, `{name} fins` for fins only; a 3MF is always the
  separate form (part, supports, the interface crest as the supports' second part, the slicer's own parts uncut beside
  the part, the part's paint, title `{name}`), with no session yet (sessions in the 3MF are M7). The triangles are the
  build's own `f64` values, not the `f32` the CLI rounds to, so a CLI-path build exports facet normals the CLI's file does not
  have; the app's builds (with `tunables_json`) are the page's, which is what this follows.
- **`export_file_name(base, kind, nozzle, layer_height) -> Result<String, FfiError>`** is the name the page suggests
  (`ui/export.js` `plateName`, `exportName`): `{base}-fins-0.4nozzle-0.2layer.stl`, `...3mf`, `{base}-fins-only-...stl`;
  a setting that is not positive is left out. `kind` is `ExportKind::{Stl, ThreeMf, FinsOnly}`.

## Load limits (hostile input, audit X-02 to X-07, S-02)

`load_parts` / `load_parts_with` run the whole parse on a thread of their own with an 8 MiB stack (the caller may be a
512 KiB cooperative-pool thread), and every limit below is checked before the work it guards.

**Always refused** (`FfiError::Format`, a message, no override): component nesting deeper than 64 ("3MF: components are
nested more than 64 levels deep"); more than 50,000 `<object>`s in one model part; JSON nested deeper than 128 (a 3MF
session is then unreadable, so the file loads as a plain part, as for any unreadable session); ZIP entries that share
bytes ("corrupt ZIP: entries overlap"); a coordinate that is not a number in an STL, OBJ, 3MF or STEP ("this STL has a
coordinate that is not a number (triangle N); the file is damaged"). A malformed Bambu `model_settings.config` no longer
panics (an `<object>` opened inside a `<part>` is read as the JS reads it).

**Load anyway** (`FfiError::LimitExceeded`; `load_parts_with(path, LoadOptions { allow_large: true })` skips them):

| `kind` | `estimate` | `limit` | hit when |
|---|---|---|---|
| `Triangles` | triangles after component expansion | 20,000,000 | an STL header, an OBJ, or a 3MF's whole build would emit more; a 3MF is counted from its object graph (memoised, so a 1 KB fan-out to 10^9 is counted, not expanded) before a triangle is emitted |
| `Bytes` | bytes the archive claims to unpack to | 2,147,483,648 (2 GiB) | the central directory's sizes (each capped at what its data could give, 1032:1) add up to more, checked before anything is inflated; a stream that outgrows its claim stops at the running budget |
| `Dimension` | the largest side of a part's box, mm | 2000 | any part of the file is wider than 2 m (checked after the parse, in `load_parts_with`, on each part's positions) |

```rust
#[derive(uniffi::Record)] struct LoadOptions { allow_large: bool /* default false */ }
#[derive(uniffi::Enum)]   enum LimitKind { Triangles, Bytes, Dimension }
FfiError::LimitExceeded { kind: LimitKind, estimate: f64, limit: f64, message: String }
```

`message` is a sentence the host can show as is ("this file expands to about 1,000,000,000 triangles (over 20,000,000)",
"this file unpacks to about 3.2 GB (over 2.1 GB)", "this model is 7,600,000 mm across (over 2,000 mm)"). The
host's flow: call `load_parts` (or `load_parts_with` with the default options); on `LimitExceeded` ask "Load anyway?"
(Cancel the default) naming the `kind` and `estimate`; on yes call `load_parts_with(path, LoadOptions { allow_large: true })`.
The constants are `sf_io::limits::{MAX_TRIANGLES, MAX_DECOMPRESSED_BYTES, MAX_DIMENSION_MM, MAX_COMPONENT_DEPTH,
MAX_JSON_DEPTH, MAX_OBJECTS_PER_PART}` and will be tuned; the host should read `limit` from the error, not hard-code it.
Estimates are what the file needs, so a refused file costs a few milliseconds and no allocation: peak RSS on every hostile
input of the audit stays under 30 MB (`crates/sf-ffi/examples/limits_probe.rs`). Reaching a limit says nothing about
whether a *build* of a large part will be fast: a 7.7 M-triangle file loads in about 1.2 s and 2.3 GB.

**The app's prompt** (track 2C): every load goes through `ViewerModel.load`, which catches `LimitExceeded`, shows an alert
on the window (title "Load <file> anyway?", a message built from `kind`, `estimate` and `limit` -- never the engine's
`message` -- and, for `Triangles`, about 280 bytes of memory per triangle; buttons Cancel (default, Return and Esc) and
Load anyway) and on yes calls `load_parts_with` with `allow_large`. A caller loading several files in turn awaits the
answer per file, so Cancel skips only that file.

**Unreadable session.** `Part.session_unreadable() -> bool` (and `sf_io::threemf::ThreeMf::session_unreadable`): the 3MF
holds a session (`Metadata/support_fins.json`) that could not be used -- damaged, nested past the JSON limit, or not
version 1. The part loads with defaults, as the JS does silently; the app shows "The settings saved in this file
couldn't be read; defaults are used." in its status banner. The import note is unchanged (parity).

The CLI takes `--allow-large` (not in `--help`, which stays the JS's text for `cli-parity`); its refusal lines end
"; pass --allow-large to load it anyway". The Rust readers: `sf_io::{stl::read_stl_with, obj::read_obj_with,
threemf::read_three_mf_unmerged_with, threemf::read_three_mf_with, zip::unzip_with}(bytes, &LoadOptions)`, returning
`ReadError::{Msg, Limit}`; the old entries keep their signatures and apply the size limits with the default options, but only the `_with` entries refuse a coordinate that is not a number (the JS carries NaN on, and the parity corpora pin it); the loader and the CLI use the `_with` ones.

## M4b additions: orientation suggestions, strength, readout data

Everything the Suggest and Strength popovers and the readouts show is computed in Rust; Swift only draws it
(`ui/suggest.js`, `ui/strength.js`, `ui/readout.js`). Wording is the page's English; the app translates later by
the same keys.

```rust
// Suggest orientation (ui/suggest.js). A ranking can take seconds on a big part, so it runs like a build.
#[derive(uniffi::Record)] struct SuggestContext { current_bore: u64, density: f64 }  // walls the CURRENT pose stands in a bore
                                                             // (summary_json `boreWalls`); the material's g/cm3
#[derive(uniffi::Record)] struct Suggestion {
    rot: Vec<f64>,          // 9, column-major: apply it to the part
    rank: String,           // "Best", "#2", "#3"
    line: String,           // "40 mm · 4 fins · 2 rough holes" or "... · can’t print (on a point)"; "40 mm" (no fin count) when fins is None
    height: f64, fins: Option<u64>, rough_holes: u64, regions: u64, bore: Option<u64>, volume: Option<f64>, // the raw numbers behind `line`
                            // fins/bore/volume are None when the ranking skipped the fin builds (a part over 1M faces, see below)
    cant_print: bool,       // seating is a point: shown as a bad row
    tier: SuggestTier,      // Free (green row, "no fins") | HoleClean (badge) | Ordinary
    badge: Option<String>,  // "Bores clean" for HoleClean
    note: Option<String>,   // the verdict's sentence for this pose
}
enum SuggestTier { Free, HoleClean, Ordinary }
#[derive(uniffi::Record)] struct SuggestResult {
    rows: Vec<Suggestion>,  // best first, at most 3
    confidence: String,     // "high" | "medium" | "low" | "none"
    note: String,           // the line under the list (empty list: why)
    note_good: bool,        // the note is the "no support" win (styled good)
}
impl Part { fn suggest_orientations(&self, threshold_deg: f64, opts_json: String, tunables_json: String, context: SuggestContext) -> Result<SuggestResult, FfiError>; } // blocking: call off the main thread
impl Part { fn suggest_orientations_wide(&self, threshold_deg: f64, opts_json: String, tunables_json: String, context: SuggestContext, density: f64) -> Result<SuggestResult, FfiError>; }
//   The wide search (M5): the same ranking, scoring and wording over `density` times the web's candidate budget
//   (10 is the wide default; 1 is exactly suggest_orientations). The candidate generator is the web's, at a finer
//   step: the cluster cap grows by `density`, the normal bucket and the near-duplicate angle shrink by sqrt(density).
//   A part with few distinct face normals (a box) has no more stable down-faces and keeps its handful of poses.

//   A part over RANK_CHEAP_FACES (1,000,000 faces; strictly more) is ranked on the analysis pass alone, with no fin build per
//   finalist (web/orient.js rankCheap), by both searches: its rows carry fins/bore/volume = None, the line leaves the fin count
//   out ("40 mm", or "40 mm · no fins" for a pose with no overhang region), "Bores clean" needs a measured bore count of 0,
//   and the confidence is capped at "medium" (one pose: "medium"; two or more: "medium" above 8% separation, else "low").

// Strength (ui/strength.js; web/orient.js loadAlignment / suggestStrengthPose)
#[derive(uniffi::Record)] struct LoadAlignment { cross: f64, quality: String /* good|mixed|poor */, text: String, can_suggest: bool }
fn load_alignment(dir_world: Vec<f64>) -> Result<Option<LoadAlignment>, FfiError>;  // world +Z is the build axis; None: no direction
#[derive(uniffi::Record)] struct StrengthPose { rot: Vec<f64>, cross: f64, quality: String, text: String, height: f64, over: f64, bed_area: f64 }
#[derive(uniffi::Record)] struct StrengthAdvice {
    pose: Option<StrengthPose>, // the pose to turn to; None when already about as strong as it gets
    already_strongest: bool,    // show `note` ("This is about the strongest printable orientation ...") instead of turning
    note: Option<String>,
}
impl Part { fn suggest_strength_pose(&self, dir_local: Vec<f64>, dir_world: Vec<f64>, threshold_deg: f64) -> Result<StrengthAdvice, FfiError>; }
//   dir_local: the load in the part's own frame; dir_world: the same load in the CURRENT pose (to compare with).

// Off the main thread, with a generation counter like Builder: a newer request drops an older, unfinished one.
#[uniffi::export(callback_interface)] trait SuggestListener: Send + Sync {
    fn suggested(&self, generation: u64, result: SuggestResult);
    fn progress(&self, generation: u64, fraction: f64); // 0..1 after every pose scored, rising only (the button's "Ranking… 40%"); never for a superseded ranking
    fn strength(&self, generation: u64, advice: StrengthAdvice);
    fn failed(&self, generation: u64, err: FfiError);
}
#[derive(uniffi::Object)] struct Suggester;                  // one per part, one worker
impl Suggester {
    fn new(part: Arc<Part>) -> Arc<Self>;
    fn suggest(&self, threshold_deg: f64, opts_json: String, tunables_json: String, context: SuggestContext, listener: Box<dyn SuggestListener>) -> Result<u64, FfiError>;
    fn suggest_wide(&self, threshold_deg: f64, opts_json: String, tunables_json: String, context: SuggestContext, density: f64, listener: Box<dyn SuggestListener>) -> Result<u64, FfiError>;
    fn strength(&self, dir_local: Vec<f64>, dir_world: Vec<f64>, threshold_deg: f64, listener: Box<dyn SuggestListener>) -> Result<u64, FfiError>;
}
// Scoring runs on Rayon's global pool (candidates in parallel, results collected in candidate order, so the ranking and its
// tie-breaks equal the serial run). A newer request stops an older ranking between candidates; the older one is never delivered.
```

`opts_json` is the same `options.json` object a build takes: the finalists are built under that gap, pad and
nozzle (the page hands the worker its `supportTunables()`). The build's request extras (`tallRibs`, `curvedWalls`,
`{"mode":"draw"}`, `sway.uncappedDepth`, `sway.crossRise`) are accepted and set aside as `Builder.build` does: they are
not clearances, so the app sends `FinSettings.optionsJSON()` unchanged. `tunables_json` is the build's too
(`FinSettings.tunablesJSON()`, the page's `finOpts().tunables`): laid over the dialog's clearances exactly as `Builder.build`
lays it (`engine_request`), so the ranking scores with the window's custom pad, rounded feet, interface and wall lines,
as the page's Worker does with `supportTunables()`. `{}` or empty text: the dialog's clearances alone. The app sends the
window's settings (`session.settings`), never a fresh `FinSettings()`. `SuggestResult` and `StrengthAdvice` carry the text
the page writes, so the row strings, badges and notes cannot drift from `ui/suggest.js`.

**Readout data added to `summary_json()`** (all computed from the build, so Swift never touches geometry):
`boreWalls` (props standing in a bore: the `SuggestContext.current_bore` of the next ranking), `seating`
(`{ kind: face|edge|point, span, bedArea }`), `hasPad`, `finVolumeMm3` and `padVolumeMm3` (the grams receipt's
volumes, `readout.js meshVolumeMM3`). `records_json()` props also carry `inBore`.

### Readout data for the status panel (`ui/readout.js`, `ui/part.js`, `ui/islands.js`, `ui/remove.js`)

Every number below is the engine value the page reads; Swift only formats it. Each is checked against the engine call
in `crates/sf-ffi/tests/readouts.rs`.

`summary_json()` also carries:
- `pad`: `null` (no pad) or `{ style: auto|light|sure|custom, autoSure, smallFoot, outline }`, `built.pad` as `padStatus`
  and `padNote` read it (`outline` is mm of first-layer edge, or `null`).
- `fill`: `null`, or Full coverage's `{ walls, bareBefore, unservedArea, capped, islandsBare, islandsCapped }`;
  `curvedFill`: `null`, or Auto's curved-walls fill `{ walls, unservedArea, capped }`; `curvedDropped`: Full kept the
  straight walls over the curves.
- `bare`: `null` or `{ area, total, coverage }`, and `fillReach`: `fillReach(bare.coverage)` in mm (the "more than {reach} mm
  from a wall" note); `null` with `bare`.
- `islandsBuild`: what the midair-starts card is told: `null` unless the mode is Full, else `{ bare, capped }` (the Full fill
  pass's `islandsBare`, `islandsCapped`) or `{ all: true }` (a Full build with no fill pass: every start is unwalled).
- `addedBounds`: `{ min: [x,y,z], max: [x,y,z] }` of the fin and pad vertices in print space (`null` if none). The page's
  `updateFit` merges it with the posed part (`analyze().size`, centred on the origin, `z` from 0) to say "does not fit (with
  fins)"; `after_removal_json` gives the same extent for the fins that are left.
- the interface crest: `records_json().crestRanges` (above); the "Flat contacts is on but no wall top got a crest" note
  (`ifaceNote`) is `crestRanges` empty with walls present.

`BuildResult.after_removal_json(removed_ids: Vec<u64>) -> String`: what changes when fins are removed by hand
(`ui/remove.js`; ids are `records_json` fin `id`s): `{ removed, bare, addedBounds, addedVolumeMm3 }`. `bare` is
`{ area, total, coverage }` measured again against the fins that are left (`bareArea` with the kept fins; the build's own
number when none are removed; `null` when the build has none); `addedBounds` is the extent and `addedVolumeMm3` the volume
(`meshVolumeMM3`, the grams receipt) of the kept fins plus the pad (`activeAdded`).

Part level:
- `AnalysisSummary` gains `raw_region_count` (regions before the sliver filter: the sliver warning compares it with
  `region_count`) and `analyze_ms`; `PartInfo` gains `weld_ms`, how long welding took at load. Together they are the
  readout's `analysisTiming` (`{analyze_ms} ms · weld {weld_ms} ms`).
- `Part.islands(threshold_deg, rot) -> Vec<Island>` (`findIslands`): `Island { x, y, z, points }`, `points` flat `x,y,z,...`;
  the count is the "midair starts" suffix, and the page draws a cone at each `x,y,z` and a line between the farthest two points.
- `island_card(count, bare: Option<u64>, capped, markers_visible) -> String`: the card text (`renderIslandCard`), empty for no
  card. `bare` is `None` without a Full build for this pose (`islandsBuild` null), else the starts left without a wall.

## M6 additions: Draw (hand-placed supports)

Everything Draw computes is Rust; Swift holds the clicks, draws the markers and the cursor. The maths is
`sf_engine::drawn` (the geometry of `web/ui/walls.js`: `sweepDrawn`, `autoSupports`, `braceAt`, `arcPoints`,
`printTriangles`) over `sf_engine::draw::drawn_wall` and `sway::sway_at_face`; `parity/oracle/ffidraw.js` slices those
functions out of walls.js at the pinned commit and records what they make, and `crates/sf-ffi/tests/draw.rs` checks the
FFI against it (counts, per-wall results and reasons, a hash of every triangle).

```rust
// One per pose and settings change: analyses the pose once, then every call below is cheap. Run it off the main thread.
#[derive(uniffi::Object)] struct DrawKit;
impl DrawKit {
    #[uniffi::constructor] fn new(part: Arc<Part>, rot: Vec<f64> /* 9, column-major */, threshold_deg: f64,
                                   settings_json: String, tunables_json: String) -> Result<Arc<DrawKit>, FfiError>;
    fn offset(&self) -> Vec<f64>;                                  // the seating offset: the part draws at rot * p + offset
    fn local_to_world(&self, p: Vec<f64>) -> Result<Vec<f64>, FfiError>;   // part.localToWorld
    fn world_to_local(&self, p: Vec<f64>) -> Result<Vec<f64>, FfiError>;   // part.worldToLocal
    fn surface_hit(&self, face: u64, origin: Vec<f64>, dir: Vec<f64>) -> Result<Option<SurfaceSpot>, FfiError>;
    fn face_takes_brace(&self, face: u64) -> Result<bool, FfiError>;              // faceIsUpright
    fn ghost(&self, a: Vec<f64>, b: Vec<f64>, m: Option<Vec<f64>>) -> Result<Arc<DrawnGhost>, FfiError>;
    fn check_wall(&self, a: Vec<f64>, b: Vec<f64>, m: Option<Vec<f64>>) -> Result<Arc<DrawnGhost>, FfiError>;
    fn sweep(&self, walls_json: String, auto_build: Option<Arc<BuildResult>>, removed: Vec<u64>) -> Result<Arc<DrawnSweep>, FfiError>;
    fn brace_at(&self, face: u64, point: Vec<f64>, sweep: Option<Arc<DrawnSweep>>, auto_build: Option<Arc<BuildResult>>,
                removed: Vec<u64>) -> Result<Arc<BraceSpot>, FfiError>;
}
#[derive(uniffi::Record)] struct SurfaceSpot { point: Vec<f64>, normal: Vec<f64>, distance: f64 }   // print space
fn arc_band(a: Vec<f64>, b: Vec<f64>, m: Vec<f64>) -> Result<Vec<f64>, FfiError>;   // arcPoints, flat x,y,z,... (<= 129 points)
```

- **`settings_json`** is what `sweepDrawn` and `swayOpts()` read off the page's controls, every key required:
  `{ tines, tineDensity (0..1: the field / 100), layerHeight, plateOnly, tallRibs, sway: { gripFrom, tineSpacing, reach (0..1: Brace
  depth % / 100), uncappedDepth, crossRise (0.1..0.9) } }`. The page divides its percent fields in JS, so a host divides them the same way
  (`x / 100`), to get the same doubles. **`tunables_json`** is the build's (`FinSettings.tunablesJSON()`): the gap, the nozzle, the wall
  lines, round feet, the interface material and the tall-rib height are read from it, as the page's modules hold them.
- **Walls** (`walls_json`) are in the part's own frame, so a wall follows the part through later turns:
  `[{ "kind": "wall", "a": [x,y,z], "b": [x,y,z], "m": [x,y,z]? }, { "kind": "sway", "a": [x,y,z], "face": n, "capped": bool }]`
  (`m`: a curve's third click; a brace's `a` is the spot it stands at, `capped` stops it at that height).
- **`surface_hit(face, origin, dir)`** is `pickFace`'s `hit.point` (web/ui/pose.js:264): the GPU id pass says which face is under the pointer,
  this says where on it (the ray meets that triangle, both sides). `origin` / `dir` (unit) are in print space (`ViewerController.ray`);
  `None` when the ray misses that triangle, the caller then keeps the last spot.
- **`sweep`** (`sweepDrawn`) re-stands every wall for the pose: `auto_build` is Auto's build when its supports are on screen (`auto` is a C keyword, which the generated header cannot use as a parameter name) (hand braces and
  tall-wall ribs stand clear of them) and `removed` its removed fin ids (`records_json` ids). `DrawnSweep`: `triangles()` (interleaved
  position + normal, `f32`, print space, in the walls' order), `walls_json()` (one entry per wall, in order: `{ ok, triStart, triEnd, kind,
  reason | length, height, tines, ribs | cross, stilt }`; `triStart` / `triEnd` are triangle indices), `pick(origin, dir, part_distance, keep)`
  (`pickSupport`: the wall under the ray if nearer than the part's own hit; `keep`, the section's plane `[nx, ny, nz, w]`, passes over hits
  where `n . p + w < 0`, as `keptHit` does), `wall_triangles(i)` (the selection's highlight).
- **`ghost`** is the live preview of the wall in progress (`updatePreview`'s `drawnWall`: `braceTall`, `through`, no tines), **`check_wall`**
  `placeWall`'s test (`drawnWall` with `through` and the topology, rot and offset: a wall whose ends sit against the part's side wall is refused at placement with "too close to the part’s side wall — draw the line a little further from it"). `DrawnGhost`: `ok()`, `reason()` (the web's text, e.g. "curve too tight - 3.0mm
  radius, needs 5mm"), `triangles()`.
- **`brace_at`** is `braceAt` (without its memo): the brace a click at `point` stands, `BraceSpot { ok(), reason(), full(), point(), key(),
  height(), triangles() }`. `key` is the web's memo key (equal keys are the same brace); `point` is what a placed brace stores. Pass the
  current `sweep` so the hand braces already standing are avoided.
- **Build mode `draw`.** `Builder.build`'s `opts_json` accepts `{"mode":"draw"}` as a request-level extra (`PageRequest.mode`, like `tallRibs`;
  `options.json` has no such mode): the build is only the seating verdict and the bed pad (`buildFins`), as the web's Draw does.
- **Export.** `export_with_drawn(path, parts, builds, drawn: Option<Arc<DrawnSweep>>, options_json)` is `export` with the hand-placed supports
  in the file, between the auto fins and the pad (`activeAdded()`, `web/ui/export.js`); `None` is `export`. `DrawnSweep.added_json()` is what
  the supports add to the plate: `{ volumeMm3, bounds: { min, max } | null }` (the fit check's extent and the grams receipt's volume).

## M6 additions: footprints on a plate (several models, Arrange)

Several models on one plate must not collide (`ui/footprint.js`, `ui/arrange.js`, `ui/models.js`, local issue 052). The
maths is `sf_engine::footprint` (a port of `ui/footprint.js` and `ui/platerow.js`, replayed against the JS in
`crates/sf-engine/tests/parity_footprint.rs`, with the 17 ported JS tests in `tests/js_footprint.rs`); `sf-ffi` only turns
a part, its pose and a build into the triangles the page rasterizes, and hands grids back as objects.

```rust
#[derive(uniffi::Object)] struct Footprint;   // the 1 mm cells of the bed a model covers at any height; never empty
impl Footprint {
  fn bounds(&self) -> Vec<i64>;                              // x0, y0, w, h in cells of `cell_size()` mm
  fn cell_size(&self) -> i64;                                // a cell's side in mm: 1, or more past CELL_BUDGET (4M cells, ~2 m square); a coarse cell is set when anything touches it, and `shared` / `clash` cells are then those cells' corners in mm
  fn area(&self) -> u64;                                     // cells set (mm²)
  fn placed(&self, x: f64, y: f64) -> Result<Arc<Footprint>, FfiError>;      // `placed`: standing at (x, y); off the mm grid it widens
  fn shared(&self, other: Arc<Footprint>) -> Result<Vec<i64>, FfiError>;     // `shared`: the cells both cover, flat x, y pairs
  fn shared_count(&self, other: Arc<Footprint>) -> Result<u64, FfiError>;
  fn dilate(&self, r: i64) -> Result<Arc<Footprint>, FfiError>;              // `dilate`: grown r cells every way
}
fn footprint_of_part(part: Arc<Part>, rot: Vec<f64>, offset: Vec<f64>) -> Result<Option<Arc<Footprint>>, FfiError>;
//   the part's mesh through the pose (9 numbers, column-major) and the seating offset (3 numbers): `partGrid`
fn footprint_of_build(build: Arc<BuildResult>, removed: Vec<u64>) -> Result<Option<Arc<Footprint>>, FfiError>;
//   what the build adds (`rasterize(flat(activeAdded()))`): the fins less the removed record ids, and the pad, f32-rounded
fn footprint_union(a: Option<Arc<Footprint>>, b: Option<Arc<Footprint>>) -> Result<Option<Arc<Footprint>>, FfiError>;
fn footprint_spot(others: Vec<Arc<Footprint>>, grid: Option<Arc<Footprint>>, bx: f64, by: f64, gap: i64) -> Result<Vec<f64>, FfiError>;
//   `spot` (ui/models.js): where a model goes on a bx x by bed, gap mm clear of `others` (placed): the free spot nearest
//   the centre, else past the others' right edge. Two numbers.
fn footprint_clash_cells(placed: Vec<Arc<Footprint>>) -> Result<Vec<i64>, FfiError>;   // the cells any two share (`checkOverlap`), x, y pairs
fn plate_under(x: f64, y: f64, active: u64, count: u64, pitch: f64, bx: f64, by: f64) -> Result<Vec<f64>, FfiError>;   // `plateUnder`: [plate, x, y] or []
fn footprint_drag_step(at: Vec<f64>, to: Vec<f64>, grid: Arc<Footprint>, others: Vec<Arc<Footprint>>) -> Result<Vec<f64>, FfiError>;
//   Move's step (`dragStep` with `clashAt` as the probe): where the model with footprint `grid` goes from `at` when the pointer asks for `to`,
//   `others` placed where they stand; the clash is the cells shared with them. Never into a worse clash; stops at the edge, slides along it. Two numbers.
```

`sf_engine::footprint` also has `pack` (Arrange's packing), ported and replayed, for Arrange to put behind the FFI when it lands; `drag_step` (Move's clamp) is `footprint_drag_step` above.

`BuildResult::kept_added(removed, drawn)` (crate-internal) is what the export writes and the footprint rasterizes: the fins
less the removed ones, the drawn walls, then the pad. `footprint_of_build` passes no drawn walls; `footprint_of_build_with_drawn(build, drawn: Option<Arc<DrawnSweep>>, removed)`
puts them in, as the page's `activeAdded()` does.

### Plate exports and Arrange (M6)

```rust
#[derive(uniffi::Record)] struct PlateModel { part: Arc<Part>, build: Option<Arc<BuildResult>>, drawn: Option<Arc<DrawnSweep>>,
    pose: Vec<f64>, threshold: f64, removed: Vec<u64>, x: f64, y: f64 }
#[derive(uniffi::Record)] struct PlateFile { name: String, models: Vec<PlateModel> }
fn export_plate(path: String, models: Vec<PlateModel>, options_json: String) -> Result<(), FfiError>;
fn export_plates(path: String, plates: Vec<PlateFile>, options_json: String) -> Result<(), FfiError>;
fn footprint_arrange(grids: Vec<Arc<Footprint>>, bx: f64, by: f64, gap: i64) -> Result<Vec<f64>, FfiError>;   // flat x, y per footprint
```

`export_plate` writes a plate of several models as one 3MF (`FORMATS['export-3mf']` over `plateGeometry`, `ui/export.js`, through
`writePlate3MF`): each model its own objects (the part, its supports with the interface crest split off, a slicer's own parts, its
paint) standing at (`x`, `y`) on the plate, titled with the first model's name; a plate of one model carries its `session`
(M7, "Sessions" below), a plate of several none. A model with a `build` carries its
fins less the `removed` record ids, its `drawn` walls, and the pad -- the same `kept_added` the single-model `export` uses; with no
`build` it is the part alone, seated from `pose` (nine numbers, column-major) at `threshold`. `options_json` is `{ "bed": [x, y] }`,
the build volume the spots are on. `export_plates` writes each `PlateFile` as a 3MF entry named `name` of one .zip (`zipStore`,
`exportAll` in `ui/plates.js`). The bytes are the page's: `parity/oracle/ffiexport.js` writes plates of one, two and three models
(fins, a fin removed, the part alone, the interface crest) and the zip through the real writers, and
`crates/sf-ffi/tests/export.rs` compares them. `footprint_arrange` is `arrange`'s placement (`sf_engine::footprint::arrange`: `pack`'s
spots, a model with no room in a row past the bed's right edge).

## M7 additions: report links (`ui/report.js`, `web/report.js`)

```rust
#[uniffi::export] fn build_report(result: &BuildResult, scale: f64, opts_json: String, tunables_json: String) -> Result<Arc<BuildReport>, FfiError>;
impl BuildReport {
  fn settings_json(&self) -> String;                                    // JSON.stringify(rep, null, 2): what Copy settings copies
  fn url(&self, site: String, when: String, drawn: u32, removed: u32) -> Result<String, FfiError>;   // reportUrl: the prefilled GitHub issue link
}
```

`result` is the build on screen; `scale` the part's scale (1 = as loaded); `opts_json` / `tunables_json` the two texts
`Builder.build` took for it. The report's `opts` is the page's `finOpts()` request rebuilt from them
(`sf_io::report::web_fin_opts`: `bedPad` is `padStyle !== 'off'`, `sway` gets `swayOpts()`'s keys with `crossAbove`
the tall-wall height or `null`, `tunables` as sent). The part's size is the box of its Float32 positions and `faces`
its triangle count; the threshold and the pose are the build's. `site` and `when` fill the "Site" line (the page
sends `location.host`; the app sends `Support Fins for Mac <engine_version>`), `drawn` is the walls drawn by hand
that built and `removed` the fins removed by hand. `crates/sf-ffi/tests/report.rs` compares both outputs with
`parity/oracle/ffireport.js`, which runs the page's `finOpts` request, `buildFins`, `reportSettings`,
`reportCoverage` and `reportUrl`, byte for byte.
## M7 additions: sessions (the 3MF carries the supports drawn by hand and the settings)

A 3MF the web exports holds a session beside the model (`Metadata/support_fins.json`, `web/ui/session.js`, #214): the drawn
walls as points, the fin mode, fins on or off, the settings and the removed fins' keys. This app writes the same file and
opens the web's, both ways. The maths is `sf_io::session` (a port of session.js: `session_of`, `part_object_of`,
`restore_session`, `form_of`), the corpus `parity/corpus/session/` (`just oracle session`: 29 sessions saved by the page's own
`sessionOf` and `writePlate3MF` and read back by its own `restoreSession`, run on the real source text of `ui/session.js`).

```rust
// reading
impl Part {
  fn has_session(&self) -> bool;                              // the 3MF this part came from carried a session still to be restored
  fn restore_session(&self, beside: bool) -> Result<SessionRestore, FfiError>;  // once, as the part arrives (afterLoad); a second call, or a part with none: why "none"
}
#[derive(uniffi::Record)] struct SessionRestore { why: Option<String> /* "changed" | "unreadable" | "none"; None: restored */,
    walls: Vec<SessionWall>, removed: Vec<String>, fin_mode: Option<String>, fins_visible: Option<bool>, form: Vec<SessionField>, import_note: String }
#[derive(uniffi::Record)] struct SessionWall { sway: bool, a: Vec<f64>, b: Vec<f64>, m: Option<Vec<f64>>, face: u64, capped: bool }
#[derive(uniffi::Record)] struct SessionField { id: String, flag: Option<bool>, text: Option<String> }
fn js_number_text(x: f64) -> Result<String, FfiError>;                          // String(x): a number setting as the page's field holds it
```

- **`load_parts`** returns just the part of a file that carries a session and whose objects are the part and its "<name> supports"
  (`partObjectOf`): the supports come back from the session, not as a mesh, and the object picker is skipped (`io.js:238-241`).
  Its import note counts all the file's objects ("imported “x” of 2 objects"), as the web's does.
- **`restore_session(beside)`** is `restoreSession`. `why` is `changed` when the part read back is not the one saved (its size
  differs by more than 0.05 mm) and `unreadable` when the session is malformed (an unknown fin mode, a wall without its points,
  ...); the part then loads as a plain part and its note says so. Otherwise `walls` are the hand-placed supports **in the part's
  own frame** (the web's centred frame shifted by the box centre, since the Rust part stays in the file's frame), a brace's `face`
  the nearest face of the part (`nearestFace`), `removed` the keys of `FinKey`, `fin_mode` / `fins_visible` the saved ones (the
  page's defaults when the file has none; `None` beside a plate, which keeps its own) and `form` the settings, each the
  `FORM_IDS` id of the web's field with its value as the field holds it once the page has settled it (`settle`: a range input
  snaps to its steps, `sway-cross-height` and `tall-ribs-height` are clamped by their change handlers). A value a field cannot
  take (a menu option that no longer exists, a blank number) is left out, keeping the app's. The pose is not stored: the
  exported part is already in that pose, so the restored pose is the identity. `beside`: added to a plate of models, whose
  settings, mode and fins visibility the part takes (`form` is empty).
- **`import_note`** is `renderImportNote` after the restore: the session clause ("restored N hand-placed supports and the
  settings", "restored the settings", the "plate keeps its settings" forms beside a plate, "saved supports not loaded: ...") is
  last. `Part::scaled` drops it (`forgetSessionNote`, `ui/scale.js`), as the rescaled part no longer has the supports.
- **Writing.** `export`'s `options_json` takes `"session": { "finMode", "finsVisible", "form": [[id, value], ...], "walls": [...],
  "removed": [...] }` (3MF only; an STL ignores it): `form` the part's settings as the web's fields hold them (strings and
  checkbox booleans) **in `FORM_IDS` order** without the viewer's ids and the nozzle, `walls` as `DrawnWall.json` writes them
  (`kind` `wall` or `sway`, in the part's own frame), `removed` the removed fins' keys. The file's session is `sessionOf`'s text
  (points relative to the box centre of the part as written, 1e-5 mm, key order as the web's), so the bytes equal the web's for
  the same state (`crates/sf-ffi/tests/session.rs`). `PlateModel.session` is the same JSON text; a plate of **one** model writes it
  (`session: gs.length === 1 ? gs[0].session() : null`, `ui/export.js`), a plate of several none. A session that is not whole is
  an `Engine` error before anything is written.
- Tested against the page three ways: the native reader restores what the page's `restoreSession` restored from the page's own
  files (`parity_session.rs`, `session.rs`); `export` writes the page's bytes for the same models; and Deno's real
  `readThreeMF` + `partObjectOf` + `restoreSession` (`parity/cli/session_check.js`, run by `session.rs`) read the files the
  native side wrote to the state they were written from.

## Crash reports (fix plan 1D; `crates/sf-ffi/src/crash.rs`)

Nothing is sent over the network; reports are files in a folder Swift names (`~/Library/Logs/Support Fins/`, the sandbox container's).

- **`crash_reports_start(dir, app_version, settings_json)`** once at launch: creates `dir`, remembers the app version and the settings,
  and installs the panic hook (once; it chains to the previous hook). Until it is called the hook writes nothing.
  **`crash_set_settings(json)`** replaces the settings text (the `{"options": ..., "tunables": ...}` of the build in force).
- **The hook** writes a report for *every* panic, including those `catch_unwind` turns into errors (the build and suggest workers):
  `crash-rust-<time>-<n>.json` and `.txt` with the message, file/line/column, `Backtrace::force_capture()` (symbolicated), the thread
  name, the operation, the app version, the engine version, the upstream pin, a UTC timestamp, the breadcrumbs and the settings.
- **Operation context:** `crash::operation("build")` returns a guard that sets a thread-local name until dropped. The entry points that
  set it: `load_parts` and `load_step_parts` ("load model"), `merge_parts`, `Part::analyze`/`islands`, the build worker ("build"), the suggest worker
  ("suggest", "suggest wide", "strength"), `export*`, `build_report`, `DrawKit::ghost`/`check_wall`/`sweep`/`brace_at` ("draw ...").
  Rayon worker threads carry none (the thread name says where it ran).
- **Breadcrumbs:** `crash_breadcrumb(text)` pushes into a ring of the last 50 (`crash_breadcrumbs()` reads it). Swift keeps its own copy
  and saves it to the folder; both end up in the reports.
- **`crash_zip(names, datas)`** is `sf_io::zip::zip_store` for Save crash report... .
- Test: `crates/sf-ffi/tests/crash.rs` forces a panic inside `catch_unwind` and reads the report back.
- **When the banner shows.** Only for a leftover running marker (the last run did not end normally) and for MetricKit crash or hang
  diagnostics. A `crash-rust-*` report alone (a panic that was caught and became an error) never raises it: the app went on. Those
  reports stay in the folder and are reachable through Help > Show crash reports.
- **Release symbols.** Checked on a Release build (`nm`, `atos`): `sf_ffi` functions (Rust v0-mangled, local symbols) are in the binary
  and `atos` resolves them, and the build also writes `Support Fins.app.dSYM`. The Rust release profile is cargo's default (no debug
  info, no strip), so a Rust frame has a function name but no file:line (the panic's own file:line is separate, from `Location`).
  A plain `xcodebuild build` does not strip (`DEPLOYMENT_POSTPROCESSING = NO`), but an Archive does with `STRIP_STYLE = all`, which
  removes every local symbol, i.e. all Rust frames. `app/project.yml` therefore sets `STRIP_STYLE: debugging` (only debug symbols go).
  To symbolicate a MetricKit address later: `atos -o "Support Fins.app.dSYM/Contents/Resources/DWARF/Support Fins" -arch arm64 -l <load address> <address>`.

## Wave 2 (October 2026): STEP out of process (X-01)

OpenCascade dereferences null or loops forever on some malformed STEP files (101 and 5 of the 106 files in
`crates/sf-step-xpc/tests/segv`), which killed the app. The app no longer links it: `sf-ffi` depends on `sf-step` (and `sf-cli`)
without the `occt` feature, and `just swift-bindings` fails if OpenCascade gets into `libsf_ffi.a` anyway.

- **The StepReader XPC service** (`app/StepReader`, in `Contents/XPCServices`, sandboxed with no other entitlement) does not
  link OpenCascade either. For each read it starts `step-read` (`app/StepRead`, in the service's `Contents/MacOS`, sandboxed
  with `inherit`: the service's sandbox, which reaches nothing but the service's container): the file's bytes and the
  `sf_step::wire` result go through two files in the service's temporary directory, removed after the read.
  `step-read` links `crates/sf-step-xpc`, `sf-step` with OpenCascade (statically) behind a C API
  (`include/sf_step_xpc/sf_step_xpc.h`). The service replies with `StepReadReply` (`app/Shared/StepReading.swift`):
  `step-read`'s exit status and, on 0, its bytes. The service itself stays up: launchd relaunches a service that exited soon
  after launch only after a 10 s penalty, which the next file would wait out (measured when the service ran OpenCascade
  itself and exited on a fault).
- **A fault** (SIGSEGV, SIGBUS, SIGILL, SIGFPE, SIGABRT, SIGTRAP) ends `step-read` with exit status 128 + the signal
  (`sf_step_xpc_exit_on_crash`, on an alternate signal stack, so a stack overflow too): no crash report, no dialog.
- **The app** (`CoreLoader.parts(at:)`): `reads_as_step`, then `StepReader.read` (a connection per read, a 120 s timeout,
  cancellable), then `load_step_parts`. A `step-read` that did not exit 0, a connection that is interrupted or invalidated, a
  proxy error or the timeout is `StepReader.Failure`: "Couldn't read this STEP file" (translated), after the usual
  `Could not read <name>:`. When the app stops waiting (timeout, cancel, quitting) it invalidates the connection and the service
  kills that connection's `step-read`, since OpenCascade may be in an endless loop.
- **OpenCascade's output** (`*** ERR StepReaderData : ...`) goes to the log (subsystem `com.storskegg.SupportFins.StepReader`,
  category `OpenCascade`), not the app's stdout (X-13), nor `step-read`'s result.
- **The CLI** still links OpenCascade and reads STEP in process (`sf-cli`'s default `occt` feature): a crash ends only the CLI.
