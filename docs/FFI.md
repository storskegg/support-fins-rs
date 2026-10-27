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
- **Cancellation (M4): by generation number.** Each new build for a part bumps a counter, and a
  finished job whose generation is stale is dropped without being delivered. Stopping a build partway
  needs cooperative checks inside the engine. That's M5, alongside parallelism.
- **Errors** are a UniFFI error enum (`FfiError::{Io, Format, Engine, Cancelled}`) carrying the JS-identical
  message where one exists.

## Surface (M4a; extended in M4b)

```rust
#[uniffi::export] fn engine_version() -> String;            // crate version + UPSTREAM commit

#[derive(uniffi::Object)] struct GpuBuffer;                  // Rust-owned, page-aligned
impl GpuBuffer { fn address(&self) -> u64; fn len_bytes(&self) -> u64; fn stride(&self) -> u32; fn count(&self) -> u32; }

#[derive(uniffi::Record)] struct PartInfo { name: String, triangles: u64, size: [f64; 3] /* as Vec<f64> */, notes: Vec<String> /* import notes, ui/io.js */ }
#[derive(uniffi::Object)] struct Part;                       // Arc<Topology> + source soup
fn load_parts(path: String) -> Result<Vec<Arc<Part>>, FfiError>;  // STL / OBJ / 3MF (one Part per object, negative parts cut out as ui/io.js does); STEP in M6
impl Part {
  fn info(&self) -> PartInfo;
  fn mesh(&self) -> Arc<GpuBuffer>;                          // interleaved position+normal, f32, model frame
  fn analyze(&self, threshold_deg: f64, rot: Vec<f64>) -> AnalysisSummary;   // rot: 9, column-major
  fn face_flags(&self, threshold_deg: f64, rot: Vec<f64>) -> Arc<GpuBuffer>; // u8 per face: over/kept/on_bed bits
}
#[derive(uniffi::Record)] struct AnalysisSummary { over_area: f64, bed_area: f64, over_face_count: u64, region_count: u64, offset: Vec<f64>, size: Vec<f64> }

// M4b:
#[uniffi::export(callback_interface)] trait BuildListener: Send + Sync { fn done(&self, generation: u64, result: Arc<BuildResult>); fn failed(&self, generation: u64, err: FfiError); }
#[derive(uniffi::Object)] struct Builder;                    // one per part; owns the generation counter + worker
impl Builder { fn new(part: Arc<Part>) -> Arc<Self>; fn build(&self, rot: Vec<f64>, opts_json: String, listener: Box<dyn BuildListener>) -> u64; }
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
- **Generations** start at 1. A job that is no longer the latest when it starts or when it finishes is dropped:
  `done`/`failed` are only called for the newest generation, on the builder's worker thread.
- **`BuildResult.fins()` / `pad()`** are `GpuBuffer`s of the same interleaved layout as `Part.mesh()` (position
  xyz + the triangle's unit normal xyz, `f32`, stride 24, three vertices per triangle) in print space, the
  seated frame the page draws. `fins()` holds walls, wedges and sway braces; `pad()` the bed pad.
- **`summary_json()`**: `{ stats, report, offset, mode, volume, tines, braceCount, propCount, unserved,
  sagRisk, smallHoleRegions, patchCount, patchStats, skipped, rejected, servedRegions, bare, floating, sway,
  tallRibs }`. `stats` and `report` are the CLI's `--json` stats and one-line report; **`patchStats` and
  `skipped` carry only the counters that counted anything** (JS leaves the others absent, #23).
- **`records_json()`**: `{ fins: [...], props: [...] }`. A `fins` entry is `{ id, kind (prop|wedge|sway), height,
  length, tines, rows, stilt, lean, bearing, span, short, fill, joined, braces?, ribs?, triRanges, line }`;
  `triRanges` are `[from, to)` **vertex** ranges into the `fins()` buffer (what per-fin removal cuts out). A
  `props` entry is `{ id, region?, tines, span, height, fill, squat, rimWall }`: **a fill-pass wall has no
  `region`** (`PropRecord.region` is an `Option`, #23).
- **`export`** writes exactly the bytes `finFile` writes (`sf_cli::cli::output_bytes`, shared with the CLI):
  the path's extension picks STL or 3MF; `options_json` is `{}` or `{ "finsOnly": true }` (an STL of the fins
  mapped back onto the part; like `--fins-only` it refuses a posed build). One part and its build per file; a
  list of several is an `Engine` error until the app has a plate layout to put them on.
