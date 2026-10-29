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
- **Errors** are a UniFFI error enum (`FfiError::{Io, Format, Engine, Cancelled}`) carrying the JS-identical
  message where one exists.

## Surface (M4a; extended in M4b)

```rust
#[uniffi::export] fn engine_version() -> String;            // crate version + UPSTREAM commit

#[derive(uniffi::Object)] struct GpuBuffer;                  // Rust-owned, page-aligned
impl GpuBuffer { fn address(&self) -> u64; fn len_bytes(&self) -> u64; fn stride(&self) -> u32; fn count(&self) -> u32; }

#[derive(uniffi::Record)] struct PartInfo { name: String, triangles: u64, size: [f64; 3] /* as Vec<f64> */, notes: Vec<String> /* import notes, ui/io.js */, import_note: String /* renderImportNote: "3MF: a; b." / "OBJ: ..." / "STEP: ..." / empty */, weld_ms: f64 }
#[derive(uniffi::Object)] struct Part;                       // Arc<Topology> + source soup
fn load_parts(path: String) -> Result<Vec<Arc<Part>>, FfiError>;  // STL / OBJ / 3MF / STEP (one Part per object, negative parts cut out as ui/io.js does). STEP (.step/.stp, or any file whose content is "ISO-10303-21;") goes through sf-step: OpenCascade 7.6.1 at 0.01 mm / 0.1 rad as web/step.js, the assembly tree picks the objects (stepObjects), names come from the file, import_note is "STEP: [imported “X” of N objects; ]tessellated at 0.01 mm." A build without OpenCascade fails the load with "STEP import is not available in this build ..."
fn merge_parts(parts: Vec<Arc<Part>>, object_count: u64) -> Result<Arc<Part>, FfiError>;  // the picker's "Merge N & load": positions concatenated in the order given (io.js mergeObjectPositions), welded as one soup; colours merged by triangle, slicer parts kept; named after the file; note "merged N of TOTAL objects into one part" (TOTAL = object_count, the file's); one part returns itself
fn scale_from_percent_text(text: String) -> Option<f64>;  // M6: scaleFromPercent(Number(text)): 10-1000 % clamped, rounded to the field's 2 decimals; None for junk, empty or <= 0
fn scale_percent_text(factor: f64) -> String;               // M6: the field's text, +(factor * 100).toFixed(2) as JS prints it ("150", "33.33")
impl Part {
  fn info(&self) -> PartInfo;
  fn mesh(&self) -> Arc<GpuBuffer>;                          // interleaved position+normal, f32, model frame
  fn analyze(&self, threshold_deg: f64, rot: Vec<f64>) -> AnalysisSummary;   // rot: 9, column-major
  fn face_flags(&self, threshold_deg: f64, rot: Vec<f64>) -> Arc<GpuBuffer>; // u8 per face: over/kept/on_bed bits
  fn scaled(&self, factor: f64) -> Arc<Part>;                // M6 Scale (ui/scale.js setScale): uniform scale about the box centre with the page's two f32 roundings (centred `f32(p - c)`, then `f32(c + s * centred)`; exact vs the oracle `parity/oracle/setscale.js`), welded afresh; slicer parts scale with it, colours/notes/name kept. Call it on the part as loaded (never on a scaled one: no compounding)
}
#[derive(uniffi::Record)] struct AnalysisSummary { over_area: f64, bed_area: f64, over_face_count: u64, region_count: u64, offset: Vec<f64>, size: Vec<f64> }

// M4b:
#[uniffi::export(callback_interface)] trait BuildListener: Send + Sync { fn done(&self, generation: u64, result: Arc<BuildResult>); fn failed(&self, generation: u64, err: FfiError); }
#[derive(uniffi::Object)] struct Builder;                    // one per part; owns the generation counter + worker
impl Builder { fn new(part: Arc<Part>) -> Arc<Self>; fn build(&self, rot: Vec<f64>, opts_json: String, tunables_json: String, listener: Box<dyn BuildListener>) -> u64; fn cancel(&self); }
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
- **`export_file_name(base, kind, nozzle, layer_height) -> String`** is the name the page suggests
  (`ui/export.js` `plateName`, `exportName`): `{base}-fins-0.4nozzle-0.2layer.stl`, `...3mf`, `{base}-fins-only-...stl`;
  a setting that is not positive is left out. `kind` is `ExportKind::{Stl, ThreeMf, FinsOnly}`.

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
    line: String,           // "40 mm · 4 fins · 2 rough holes" or "... · can’t print (on a point)"
    height: f64, fins: u64, rough_holes: u64, regions: u64, bore: u64, volume: f64, // the raw numbers behind `line`
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
impl Part { fn suggest_orientations(&self, threshold_deg: f64, opts_json: String, context: SuggestContext) -> Result<SuggestResult, FfiError>; } // blocking: call off the main thread
impl Part { fn suggest_orientations_wide(&self, threshold_deg: f64, opts_json: String, context: SuggestContext, density: f64) -> Result<SuggestResult, FfiError>; }
//   The wide search (M5): the same ranking, scoring and wording over `density` times the web's candidate budget
//   (10 is the wide default; 1 is exactly suggest_orientations). The candidate generator is the web's, at a finer
//   step: the cluster cap grows by `density`, the normal bucket and the near-duplicate angle shrink by sqrt(density).
//   A part with few distinct face normals (a box) has no more stable down-faces and keeps its handful of poses.

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
    fn strength(&self, generation: u64, advice: StrengthAdvice);
    fn failed(&self, generation: u64, err: FfiError);
}
#[derive(uniffi::Object)] struct Suggester;                  // one per part, one worker
impl Suggester {
    fn new(part: Arc<Part>) -> Arc<Self>;
    fn suggest(&self, threshold_deg: f64, opts_json: String, context: SuggestContext, listener: Box<dyn SuggestListener>) -> u64;
    fn suggest_wide(&self, threshold_deg: f64, opts_json: String, context: SuggestContext, density: f64, listener: Box<dyn SuggestListener>) -> u64;
    fn strength(&self, dir_local: Vec<f64>, dir_world: Vec<f64>, threshold_deg: f64, listener: Box<dyn SuggestListener>) -> u64;
}
// Scoring runs on Rayon's global pool (candidates in parallel, results collected in candidate order, so the ranking and its
// tie-breaks equal the serial run). A newer request stops an older ranking between candidates; the older one is never delivered.
```

`opts_json` is the same `options.json` object a build takes: the finalists are built under that gap, pad and
nozzle (the page hands the worker its `supportTunables()`). `SuggestResult` and `StrengthAdvice` carry the text
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
    fn face_takes_brace(&self, face: u64) -> bool;                // faceIsUpright
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
  reason | length, height, tines, ribs | cross, stilt }`; `triStart` / `triEnd` are triangle indices), `pick(origin, dir, part_distance)`
  (`pickSupport`: the wall under the ray if nearer than the part's own hit), `wall_triangles(i)` (the selection's highlight).
- **`ghost`** is the live preview of the wall in progress (`updatePreview`'s `drawnWall`: `braceTall`, `through`, no tines), **`check_wall`**
  `placeWall`'s test (`drawnWall` with `through` only). `DrawnGhost`: `ok()`, `reason()` (the web's text, e.g. "curve too tight - 3.0mm
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
  fn bounds(&self) -> Vec<i64>;                              // x0, y0, w, h in cells
  fn area(&self) -> u64;                                     // cells set (mm²)
  fn placed(&self, x: f64, y: f64) -> Arc<Footprint>;        // `placed`: standing at (x, y); off the mm grid it widens
  fn shared(&self, other: Arc<Footprint>) -> Vec<i64>;       // `shared`: the cells both cover, flat x, y pairs
  fn shared_count(&self, other: Arc<Footprint>) -> u64;
  fn dilate(&self, r: i64) -> Arc<Footprint>;                // `dilate`: grown r cells every way
}
fn footprint_of_part(part: Arc<Part>, rot: Vec<f64>, offset: Vec<f64>) -> Result<Option<Arc<Footprint>>, FfiError>;
//   the part's mesh through the pose (9 numbers, column-major) and the seating offset (3 numbers): `partGrid`
fn footprint_of_build(build: Arc<BuildResult>, removed: Vec<u64>) -> Option<Arc<Footprint>>;
//   what the build adds (`rasterize(flat(activeAdded()))`): the fins less the removed record ids, and the pad, f32-rounded
fn footprint_union(a: Option<Arc<Footprint>>, b: Option<Arc<Footprint>>) -> Option<Arc<Footprint>>;
fn footprint_spot(others: Vec<Arc<Footprint>>, grid: Option<Arc<Footprint>>, bx: f64, by: f64, gap: i64) -> Vec<f64>;
//   `spot` (ui/models.js): where a model goes on a bx x by bed, gap mm clear of `others` (placed): the free spot nearest
//   the centre, else past the others' right edge. Two numbers.
fn footprint_clash_cells(placed: Vec<Arc<Footprint>>) -> Vec<i64>;   // the cells any two share (`checkOverlap`), x, y pairs
fn plate_under(x: f64, y: f64, active: u64, count: u64, pitch: f64, bx: f64, by: f64) -> Vec<f64>;   // `plateUnder`: [plate, x, y] or []
fn footprint_drag_step(at: Vec<f64>, to: Vec<f64>, grid: Arc<Footprint>, others: Vec<Arc<Footprint>>) -> Vec<f64>;
//   Move's step (`dragStep` with `clashAt` as the probe): where the model with footprint `grid` goes from `at` when the pointer asks for `to`,
//   `others` placed where they stand; the clash is the cells shared with them. Never into a worse clash; stops at the edge, slides along it. Two numbers.
```

`sf_engine::footprint` also has `pack` (Arrange's packing), ported and replayed, for Arrange to put behind the FFI when it lands; `drag_step` (Move's clamp) is `footprint_drag_step` above.

`BuildResult::kept_added(removed, drawn)` (crate-internal) is what the export writes and the footprint rasterizes: the fins
less the removed ones, the drawn walls, then the pad. `footprint_of_build` passes no drawn walls until Draw's app side lands
(the page's `activeAdded()` counts them).
