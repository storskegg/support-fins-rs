# Audit: concurrency, thread safety and resource use (C-NN)

Worktree: `~/RustRoverProjects/sfrs-wt/audit-concurrency` (detached at origin/main d2eb914). The probes are left there and are not for merging:
- `crates/sf-ffi/examples/audit_stress.rs`: the stress harness (generators, build/drag/cancel, Suggest, Strength, Sway hover, export, leak loop, plate).
- `crates/sf-ffi/examples/audit_qos.rs`, `audit_one.rs`, `audit_leak.rs`.
- `app/SupportFinsTests/AuditLeakTests.swift`: window lifetime tests.
- `crates/sf-ffi/Cargo.toml`: adds rayon as a dev-dependency, for the QoS probe only.

**Tools**
- rustc 1.99.1 (pinned), and nightly rustc 1.101.0-nightly (92ffebece 2026-10-28) with rust-src (installed) for `-Zsanitizer=thread|address -Zbuild-std`.
- Xcode xcodebuild with `-enableThreadSanitizer YES`; XcodeGen.
- `getrusage` / `ps` for RSS, CPU and threads.
- The macOS DiagnosticReports `.ips` files, which I read but did not create.

I triggered no crash dialogs; all the crash evidence below comes from reports that already existed.

## Summary

| ID | Title | Severity | Confidence |
|---|---|---|---|
| C-01 | Every window that showed a part leaks: the ViewerModel, its Part, builds, Metal buffers and worker threads | High | Confirmed |
| C-02 | `unowned let host` in 25 Feature/model classes, plus Tasks that outlive the window, give `swift_abortRetainUnowned` crashes. C-01 hides them today; fixing C-01 alone exposes them | High | Confirmed (crash reports) |
| C-03 | Export (STL/3MF/plate) runs synchronously on the main actor: 4.3 s (EF object) and 7.4 s (1.2M-face) beachballs | Medium | Confirmed |
| C-04 | A Suggester is never cancelled: rankings and Strength searches run on after their window, part or popover is gone | Medium | Confirmed |
| C-05 | Loading the private 3MF peaks at 4.8 GB RSS (10 objects, 7.7M faces), and each object becomes a GPU mesh before the picker shows | Medium | Confirmed |
| C-06 | A Rust panic in 82 non-throwing FFI exports aborts the app (`try!`). A panic in a listener callback kills the worker thread silently, so builds never land again | Low | Confirmed (code) / Possible (trigger) |
| C-07 | A BuildResult stores the fin geometry three times (GPU soup, f32 copy, deep clone of FinsBuilt) | Low | Confirmed (code) |
| C-08 | Main-thread engine calls on click (`braceAt`, `checkWall`, `sweepNow`) take up to 118 ms (EF) or 294 ms (1.2M faces) | Low | Confirmed |
| C-09 | The "User-interactive waiting on Utility" priority inversion: Rust threads run at Default QoS, the only Utility work is ParkedWalls, and the report is most likely from the UI-test runner | Info | Possible |
| C-10 | Swift 6 complete checking already on: 3 distinct concurrency warnings remain | Info | Confirmed |
| C-11 | Per-thread `SEEN` stamp arrays (4 B × faces) stay on every Rayon worker for the life of the process | Info | Confirmed (code) |

Totals: High 2, Medium 3, Low 3, Info 3.

## Measurements

The machine was an M2 Max (12 cores). Load figures come from `tools/lock/lock.py --bench`.
- Main stress run: baseline 1-minute load **4.47** (mean of 6 samples), mean **15.53** during the run. The run shared the machine with other agents' builds.
- Individual lines print `vm.loadavg` at their start.

Release build, `audit_stress`. "cpu" is user+sys of the whole process over the step. Builds use `build_part` with `"{}"` options: the app's CLI path, which re-welds the posed part every build.

**Earth Focus Many Vertical.3mf, object 0 (772,340 faces).** Load at start: 7.0 / 8.3 / 16.4.

| step | wall | CPU (cores) | RSS after | peak RSS | threads | note |
|---|---:|---:|---:|---:|---:|---|
| load_parts (all 10 objects, 7.72M faces) | 1.58 s | 4.6 s (2.9) | 1566 MB* | **4772 MB** | 13 | *after dropping objects 1-9 |
| build cold / warm | 710 / 598 ms | 2.2 / 2.5 cores | 1621 MB | | 13 | |
| drag: 200 superseding builds at 8 ms + cancel every 10th, then the release build (×3) | 2.49-2.54 s | ~3 cores | 1634-1647 MB | | 14 | release build landed 429-495 ms after it was asked for; no thread growth |
| cancel in flight, then rebuild | | | | | | landed 621 ms after the cancel (≈ one build) |
| Suggest default | 1.64 s | 2.7 cores | | | 14 | 19 progress callbacks |
| Suggest wide ×10 | 2.64 s | 1.6 cores | | | 14 | 139 progress callbacks |
| wide superseded by Strength | | | | | | Strength landed 962 ms after the request (773 ms alone): ~190 ms to stop the ranking |
| Strength alone | 773 ms | 1.2 cores | | | | |
| build while an orphaned wide ranking runs | 1144 ms | | | | 15 | 642 ms alone; the orphan reported for another 969 ms (C-04) |
| analyze / DrawKit::new, idle pool vs during wide Suggest | 6.7 / 11.3 ms median vs 7.3 / 11.9 (max 15.1) | | | | | no significant Rayon starvation |
| Sway hover `brace_at` ×40 upright faces | 212 ms total | | | | | first 52.5 ms, median <0.1 ms, max 118 ms |
| `surface_hit` ×40 | <0.01 ms each | | | | | |
| export STL (38.7 MB) / 3MF (54.4 MB) | 35 ms / **4264 ms** | | | | | the 3MF runs on the main actor in the app (C-03) |
| repeated builds (`audit_leak`, 100 each: build_part / one Builder / new Builder each) | | | 1179 / 1233 / 1191 MB at the end | | | RSS falls from 1610 MB and flattens: **no Rust leak** |

**Generated 1.2M-face flat torus** (`gen-torus 1000 600`). Load at start: 14.3 / 10.0 / 16.6.

| step | wall | CPU (cores) | RSS | peak | threads | note |
|---|---:|---:|---:|---:|---:|---|
| load | 145 ms | 2.4 | 260 MB | 260 MB | 13 | |
| build cold / warm | 3151 / 3416 ms | **0.9 / 0.8** | 994-1135 MB | 1188 MB | 13 | the build barely uses the pool on this part |
| drag ×3 | 3.0-3.8 s | 1.0-1.5 | 1059-1090 MB | | 14 | release landed after 1741 / 1428 / 997 ms |
| Suggest default / wide | 1112 / 1664 ms | | 1250 / 627 MB | **1890 MB** | 14 | over RANK_CHEAP_FACES: analyze-only ranking; wide adds ~640 MB transient |
| Strength | 1489 ms | 0.9 | | | | |
| orphan wide while building | 2334 ms (2933 alone) | | | | | the orphan stopped within 300 ms (cheap pass) |
| analyze / DrawKit::new during wide | 42.8 / 46.8 ms median vs 21.1 / 31.5 idle | | | | | ~2× slower, still short |
| first `brace_at` | 294 ms | | | | | builds the triangle index |
| export STL / 3MF | 50 ms / **7368 ms** | | 1649 MB | | | |
| leak loop, 20 × (Builder + 20 builds) | | | 1476 → 1586 → 1396 → 1463 MB | | 13 | not monotonic: allocator, not a leak |

**Plates (one Builder per object, all rebuilt at once).**

| case | load at start | wall (first / last landed) | CPU (cores) | RSS | peak | threads |
|---|---|---:|---:|---:|---:|---:|
| 24 generated 80 mm towers × 40k faces (960k), round 0 / 1 / 2 | 17.9 | 99 / 228, 131 / 556, 216 / 784 ms | 7.8 / 6.0 / 3.6 | 299-530 MB | 590 MB | **37** (24 `sf-build`) |
| same, 20 settings changes in 200 ms | | 707 ms | 5.8 | 563 MB | | 37 |
| EF, all 10 objects (7.72M faces), 3 rounds | 18.4 | 1314 / 1999, 1102 / 1974, 837 / 1622 ms | 6.6-7.2 | 2.5-2.8 GB | 4.78 GB (load) | 23 |
| EF, 20-change storm | | 2120 ms | 7.5 | 2.5 GB | | 23 |

Nothing exploded: one OS thread per Builder or Suggester, and the global Rayon pool stays at 12. Rapid rebuilds and cancels never grew the thread count or RSS.

**App windows (`AuditLeakTests`, Debug test host).**
- An empty window is freed.
- A window that showed a part is never freed: 0/10 small windows, 0/5 big windows.
- Resident memory after closing each of five windows holding a 320k-face sphere with fins: 213 → 409 → 526 → 642 → 755 → 869 MB, about **+115 MB per closed window**. The same test under TSan: 566 → 1555 MB.

**Sanitizers**
- Rust ThreadSanitizer (nightly, `-Zbuild-std`):
  - sf-ffi `builder` (7), `suggest` (16) and `draw` (8) tests: 0 reports.
  - The `audit_stress` run on a tower: build, 3 × 200-rebuild drag with cancels, Suggest, supersede, orphan, starve, Sway, 20-builder leak loop. 0 reports.
  - Instrumentation was verified (`__tsan_*` symbols, the tsan runtime linked).
  - The Manifold/OCCT C++ is not instrumented.
- Rust AddressSanitizer:
  - sf-ffi `builder`, `suggest`, `draw`, `export`, `footprint` and `session` tests: 0 reports.
  - `audit_stress` (build, drag, Suggest, Sway, export): 0 reports. This covers `GpuBuffer`'s raw allocation and the C++ shims' Rust side.
- Swift ThreadSanitizer (`xcodebuild -enableThreadSanitizer YES`): 65 tests in 8 suites passed with **0 TSan warnings**. The suites were BuildController, BuildModel, Window wiring, Draw and several models, Plate board, Strength arrow, Suggest orientation and the audit leak tests.
- Swift ASan: not run (time).
- Miri: not run (no Miri component; the only `unsafe` is `GpuBuffer` and the C FFI, which ASan covered).

---

## C-01 Every window that showed a part leaks (ViewerModel, Part, builds, Metal buffers, threads)
**Severity:** High · **Confidence:** Confirmed

**Location:**
- `app/SupportFins/Suggest/SuggestFeature.swift:29-35`: `toolbar.register(.suggest) { [host] in ... Self.rank(suggest, in: host) }`. The comment says it "captures the host as it always has (other features' `unowned` references rely on the window living on)".
- The cycle is: ViewerModel → `toolbar` (ToolbarRegistry) → registered closure → `host` (strong).

**Evidence**
- `AuditLeakTests` (app unit tests, run under the lock): `empty window freed: true`; `window with a part freed: false, part freed: false`; `window with fins freed: false … build model freed: false`; `10 windows: freed 0/10`; `5 big windows: freed 0/5`.
- Resident MB after each close: [409, 526, 642, 755, 869] from 213, about 115 MB per 320k-face window.
- `registerTools` runs in `partDidShow` whenever there is a controller, so every window that ever showed a part is caught. That includes a multi-object file, where `SuggestModel(part: nil)` is still non-nil.

**Impact**
- Closing a window frees nothing. Each closed window keeps its Part (topology, f64 positions and caches), the latest BuildResult (C-07), plate models and their builds.
- It also keeps the Metal mesh, flag and fin buffers (no-copy over Rust memory) and three idle OS threads (`sf-build` and two `sf-suggest`, Suggest and Strength), plus one `sf-build` per parked plate model.
- On the private file a session of open, close and reopen grows by gigabytes. The process is never reclaimed until quit.

**Recommendation (M, no parity impact)**
- Capture `[weak host]` (or `[unowned host]` only together with C-02) in the Suggest toolbar closure, then rerun `AuditLeakTests`. The test only proves the window is retained; once Suggest is fixed, check for other cycles the same way.
- Do this together with C-02, never alone: freeing the window turns today's leak into the unowned crashes of C-02.
- Promote the audit's weak-reference tests (an empty window, a part, fins, several windows) to a permanent app test.

## C-02 `unowned let host` + Tasks that outlive the window give `swift_abortRetainUnowned` crashes (masked by C-01)
**Severity:** High · **Confidence:** Confirmed (crash reports from test hosts)

**Location**
- `unowned let host: ViewerModel` in 25 classes: every `*Feature`, `MoveModel`, `DrawModel`, `ReportModel` and `PlateBoard` (`app/SupportFins/Plates/PlateBoard.swift:28`; `var session: PartSession { host.session }`, line 94).
- `MoveModel.swift:36-39` closures use `[unowned self]`.
- `DrawModel.swift:204-205` uses `[weak draw, unowned board]`.
- The fatal pattern is in `app/SupportFins/Plates/PlateBoard+Parked.swift:35-56`:
  - `parkedTask = Task { [weak self] in await self?.buildParked() … }`.
  - `await self?.buildParked()` keeps the board alive across `await s.build.settled()`.
  - The loop then reads `session`, which reaches the dead `host`.

**Evidence:** `~/Library/Logs/DiagnosticReports/Support Fins-2026-10-29-1201xx.ips`. These are 10 test-host crashes today, all with `EXC_CRASH SIGABRT`:
- 9 have the stack `swift_abortRetainUnowned < swift_unownedRetainStrong < PlateBoard.session.getter < PlateBoard.buildParked() < closure #1 in PlateBoard.pumpParked()`.
- 1 has `PlateBoard.go(_:quiet:) < addPlate()`.

Earlier reports show the same abort:
- 10-29 12:01:00: in `DrawModel.controller.getter < DrawModel.pointerClicked`. This is the DrawModel crash in the seed notes.
- 10-28: in `SectionFeature.menus`, `RotateFeature.menus` and `ViewerRenderer.init closure` (`swift_unknownObjectUnownedLoadStrong`).
- 10-27: in `FakePose.access`.

These are test hosts on dev branches, but the PlateBoard code that crashed is unchanged on origin/main. In the app, C-01 keeps every host alive, which is the only reason users do not see these crashes.

**Impact:** with C-01 fixed, closing a window while a parked plate build, a debounced task, a Draw kit task or a menu refresh is pending aborts the whole app (every window is lost).

**Recommendation (M, no parity impact)**
- Make `host` `weak` (or route features through a weak `HostRef`) and treat a nil host as "window closed".
- In long-running Tasks, re-take `guard let self` after each `await` instead of `await self?.method()` that loops internally.
- Cancel `parkedTask`/`parkedTimer`, the BuildController tasks and the Draw tasks from a window-close hook.
- Land with C-01 and a test that closes a window during a parked build.

## C-03 Export runs synchronously on the main actor (4-7 s beachball on big parts)
**Severity:** Medium · **Confidence:** Confirmed

**Location**
- `app/SupportFins/Model/ExportModel.swift:181-198` (`@MainActor`, `write` → `exportWithDrawn`).
- `app/SupportFins/Plates/PlateBoard+Export.swift:107-160` (`writePlate` / `writeAll` → `exportPlate`/`exportPlates` on the MainActor; the `async` functions await only the settle).
- `DrawModel.sweepNow()` (`DrawModel+Readout.swift:39-48`) and `ParkedWalls.sweepNow(for:)` build a `DrawKit` (analyze) and run a full `sweep` on the main actor during a plate export.

**Evidence (audit_stress, Rust only)**

| part | STL export | 3MF export |
|---|---:|---:|
| EF object 0 | 35 ms | 4264 ms |
| 1.2M torus | 50 ms | 7368 ms |
| 40k tower | 3.7 ms | 448 ms |

A plate export of the 10 EF objects writes all of them in one main-thread call.

**Impact:** the window and the whole app freeze (spinning cursor) for seconds on every 3MF or plate export of a large part, longer for "All plates".

**Recommendation (S-M, no parity impact)**
- Snapshot the inputs on the main actor, then run `exportWithDrawn` / `exportPlate(s)` (and the `sweepNow` fallback) in `Task.detached(priority: .userInitiated)`. The FFI objects are `Sendable`.
- Show progress, and keep `busy` set until the write returns.
- Separately, profile why the 3MF writer is about 100× the STL time (zip deflate of the XML?). That belongs to the perf area; byte output must stay identical.

## C-04 Suggesters are never cancelled: rankings and Strength searches outlive their window, part or popover
**Severity:** Medium · **Confidence:** Confirmed

**Location**
- `crates/sf-ffi/src/suggest.rs:487-560`.
- `Suggester` has no `cancel()` and no `Drop` that bumps `generation`. The ranking's stop test is `latest != job.generation`, so dropping the Suggester (the sender) never stops the job in flight. The worker also drains every queued job that is still "latest".
- `Work::Strength` has no stop test at all.
- Pass 2 of `orient.rs:rank_downs` (lines 547-560) checks `cancelled()` only before each finalist's full `build_fins` (non-cancellable). A ranking that has entered pass 2 stops only after the finalist builds already started.
- Swift: `SuggestModel`/`StrengthModel` are replaced on `partDidShow` (`SuggestFeature.swift:21`, `StrengthFeature.swift:20`) without stopping anything.
- Builder, by contrast, is cancelled on part change (`BuildWiring.attach` → `setEnabled(false)` → `driver.cancel()`). But `Builder` also lacks a `Drop` that raises the in-flight flag, so dropping it mid-build (window close, parked model discarded) lets the last queued job run to the end.

**Evidence**
- EF object 0: after dropping a Suggester 300 ms into a wide ranking, the orphan kept reporting progress for another 969 ms. A build started meanwhile took 1144 ms against 642 ms alone (1.8×).
- Superseding a wide ranking by Strength: Strength landed 962 ms after the request against 773 ms alone, so the ranking took about 190 ms to notice.
- 1.2M torus: the orphan stopped within 300 ms (cheap-only path).
- Comment at `orient.rs:336`: finalist builds are "~40 s on a 2.3M-face mask". Just under `RANK_CHEAP_FACES` (1M) a stale ranking can therefore hold the pool for tens of seconds.

**Impact:** after opening a new file or closing the window while Suggest or Strength runs, the CPU stays busy and the new part's build and overhang shading slow down. With C-01 fixed, an orphan delivers into a dead model, which is harmless because the callbacks are weak.

**Recommendation (S, no parity impact)**
- Add `Suggester::cancel()` (bump the generation) and `impl Drop for Suggester`/`Builder` that bumps the generation and raises the in-flight flag.
- Give `Work::Strength` the same staleness checks.
- Call `cancel()` from `SuggestModel`/`StrengthModel` when replaced and on window close.
- Optionally pass the build Cancel into pass-2 `build_fins_cancellable`. The result is unchanged when not cancelled, so the bench_m5 hashes are untouched.

## C-05 Loading the private 3MF peaks at 4.8 GB; every object becomes a GPU mesh before the picker
**Severity:** Medium · **Confidence:** Confirmed

**Location**
- `crates/sf-ffi/src/part.rs:717-800` (`load_parts` builds a `Part` with a full topology for every object).
- `app/SupportFins/CorePart.swift:95-104` (`CoreLoader.load` makes a `CorePart`, which calls `part.mesh()`, a 72 B/face page-aligned copy, for every object before `ViewerModel` shows the picker).

**Evidence**
- `load_parts` on the private file (10 objects, 7,723,400 faces) has a peak RSS of 4772 MB in 1.4-1.6 s.
- 1566 MB is still resident after dropping 9 of the 10 parts, partly allocator high-water.
- The app then adds 7.72M × 72 B ≈ 556 MB of mesh buffers for objects the user may not pick.

**Impact:** an 8 GB Mac swaps hard when opening that file. Opening it twice in two windows doubles it, and with C-01 the memory is never returned.

**Recommendation (M, parity-neutral if the outputs are unchanged)**
- Read the objects' triangle soups first and weld (build the topology) only for the objects the picker chooses; the picker needs only counts and bounds.
- Defer `mesh()` until a part is shown.
- Measure peak RSS with `bench_load` afterwards.

## C-06 Panics across the FFI: 82 non-throwing exports abort; a panicking listener callback silently kills the worker
**Severity:** Low · **Confidence:** Confirmed (code) / Possible (no panicking input found)

**Location**
- UniFFI 0.32.2 catches panics in the scaffolding, but the generated Swift (`sf_ffi.swift`) calls non-throwing functions as `try! rustCall`, which aborts the app. 82 sites do this.
  - Examples: `Part.scaled`, `Part.mesh`, `restoreSession`, `DrawKit.faceTakesBrace`, every `footprint_*` (`dilate(r:)`, `footprintSpot(gap:)`), `BuildResult.recordsJson`, `Builder.build`.
- For throwing exports, a panic becomes `UniffiInternalError.rustPanic`, which the app's `try?` swallows silently (`surfaceHit`, `braceAt`, `sweep`).
- `crates/sf-ffi/src/builder.rs:466-470` and `suggest.rs:549-553`: the `listener.done/failed/suggested` calls are outside `catch_unwind`. If a callback panics (UniFFI panics when a foreign callback reports an unexpected error), the `sf-build`/`sf-suggest` thread dies and drops the receiver. Every later `build()` is then silently discarded (`let _ = self.jobs.send(..)`): the spinner stays up and `settled()` never returns.
- Also: progress is reported under a `Mutex` across the FFI call (`suggest.rs:516-522`, `sent.lock().unwrap()`). It is harmless while callbacks don't panic, but it would poison the lock.

**Evidence:** code reading and the generated bindings. ASan and TSan runs found no panics on real inputs, and I found no panicking input (input validation belongs to the security audit).

**Recommendation (S)**
- Wrap the listener calls in `catch_unwind`, and make `build()`/`submit()` respawn the worker or return an error if `send` fails.
- Make the engine-heavy non-throwing exports throwing (`scaled`, `mesh`, footprints with caller-sized grids) or clamp their inputs.
- Log `rustPanic` instead of dropping it in `try?`.

## C-07 A BuildResult stores the fins three times
**Severity:** Low · **Confidence:** Confirmed (code)

**Location:** `crates/sf-ffi/src/builder.rs:40-62` and `finish()` (~395-420). Each BuildResult holds:
- the `fins`/`pad` GpuBuffers (interleaved position+normal f32, 72 B/triangle);
- `triangles: triangles.to_vec()` (36 B/triangle);
- `built: built.clone()`, a deep copy of `FinsBuilt` with its f64 `V4` triangles (96 B/triangle) and records.

`soup_buffer` also allocates a temporary Vec before copying into the page-aligned buffer.

**Impact:** about 200 B per fin triangle stays alive for every BuildResult the app holds (the live model, each parked plate model, and every leaked window under C-01), plus transient doubling at the end of every build. On EF, RSS moves by roughly 40-50 MB per held build.

**Recommendation (S, no parity impact)**
- Move `built` into `finish` instead of cloning.
- Derive the export triangles from `built` on demand, or keep only the f32 soup and rebuild normals for the GPU in place.
- Write `soup_buffer` straight into the `GpuBuffer`.

## C-08 Click-time engine calls on the main actor take up to 118-294 ms
**Severity:** Low · **Confidence:** Confirmed

**Location**
- `DrawModel.placeBrace` (`DrawModel.swift:716-731`, `kit.braceAt`).
- `placeWall` (`:697-712`, `kit.checkWall`).
- `sweepNow` (C-03), all on the main actor.
- Hover is already off the main thread (`braceTask`/`ghostTask` use `Task.detached`), and `surfaceHit` is under 0.01 ms.

**Evidence:** `brace_at` first call (it builds the triangle index) took 52.5 ms on EF and 294 ms on the 1.2M torus, max 118 ms on EF; the median is under 1 ms.

**Impact:** a visible hitch on the first brace click on a big part, and an unresponsive UI while `checkWall` runs on a dense mesh.

**Recommendation (S):** reuse the hover's detached result when its key matches the click (it usually does), or run the click's call detached and apply it on return.

## C-09 The reported QoS priority inversion
**Severity:** Info · **Confidence:** Possible

**Evidence**
- `audit_qos` probe: Rayon workers run at **Default** QoS whether the pool was first touched from a Utility or a User-interactive thread, and so does a `std::thread` spawned from a UI thread. `sf-build`/`sf-suggest` are therefore Default, never Utility.
- The app's only Utility work is `ParkedWalls` (`Task.detached(priority: .utility)` → `DrawKit` + `sweep`, which takes the topology's `Mutex`/`OnceLock` caches: `seated`, `sway_patches`, `near_index`).
- Main-actor paths that can block on the same part's caches are `sweepNow(for:)` (export) and the click calls (C-08). That is a real but narrow in-app inversion: it needs a plate with parked models with walls.
- The Suggest UI test has no ParkedWalls activity, so the warning there most likely comes from the XCUITest runner's own main thread. `PartWindowUITestCase` uses `Thread.sleep` and `XCTWaiter().wait`.
- I could not reproduce it: UI tests are off limits, and Swift TSan unit tests printed no QoS warnings.

**Recommendation (S):** give `ParkedWalls` `.userInitiated` (it feeds the visible plate), and move the main-actor `sweepNow` off the main actor (C-03). Then, if the warning still appears, capture it with the Thread Performance Checker on an app run.

## C-10 Swift 6 strict concurrency: 3 distinct warnings
**Severity:** Info · **Confidence:** Confirmed

The project already builds with `SWIFT_VERSION 6.0` and `SWIFT_STRICT_CONCURRENCY: complete` (`app/project.yml:17-18`); the packages use language mode v6. `xcodebuild build-for-testing` gives 50 warnings in all. The concurrency ones, deduplicated:
- 2 × "main actor-isolated static property … can not be referenced from a Sendable closure":
  - `SearchCatalog.swift:83` (`TinesSection.pinSpacingTip`);
  - `SearchCatalog.swift:133` (`MoreSettingsSection.coverageTipFull`).
- 1 × `#MutableGlobalVariable` in `SupportFinsTests/FeatureSetTests.swift:53` (`probeTypes`).

The rest are 32 deprecations of `Text +` and 2 others.

`@unchecked Sendable` is used only on the listener sinks (immutable `@Sendable` closures), `KeyMonitor` and `ToolbarSlot`. All were reviewed and are sound.

**Recommendation (S):** mark the two tip statics `nonisolated` (they are pure `JS.t` lookups) and make `probeTypes` a `let` in a `@MainActor` scope.

## C-11 Per-thread stamp arrays stay on Rayon workers for the process's life
**Severity:** Info · **Confidence:** Confirmed (code)

**Location:** `crates/sf-engine/src/inside.rs:386-400`, `thread_local! SEEN: (Vec<i32>, i32)`. It grows to the largest part's `n_faces` and never shrinks, on each of 12 Rayon workers plus the build threads.

**Impact:** about 4 B × faces × 13 threads is kept after the part is closed: about 40 MB for a 772k-face part, about 400 MB after a 7.7M-face merge.

**Recommendation (S, no parity impact):** shrink or free the arrays when a Topology is dropped (a generation counter), or key them per topology.

---

## What's solid (don't churn)
- **Cancellation core** (`sf-engine/src/cancel.rs`): a relaxed flag, unwound with `resume_unwind(Cancelled)` and caught at one entry. Rayon propagates it, and `catch` re-raises foreign panics. Poll sites never hold a `Mutex` (checked every engine `.lock()`: the critical sections are pure computation with no poll and no Rayon join), so cancellation cannot poison the topology caches.
- **`lazy::get_or_build`**: the OnceLock-inside-Rayon deadlock is designed out, with a debug assertion.
- **`Builder`**: a generation counter plus per-job cancel flags swapped under one lock. Stale jobs are skipped before and after running, and Swift re-checks `g == wanted`. TSan was clean through 3 × 200-rebuild drags with cancels. Thread count is flat and there is no RSS growth on EF over 100 builds in any mode.
- **Swift delivery:** the listener and sink callbacks only hop to the main actor (`Task { @MainActor … }`, `[weak self]`), so no Rust thread ever waits on the main thread; deadlock is impossible here. Progress ordering is handled with `max`.
- **`ViewerController.runFlags`**: one analysis in flight, newest pose wins.
- **`GpuBuffer`** + `metalBuffer(device:)`: page-aligned, and the MTLBuffer deallocator keeps the Rust owner alive. ASan was clean.
- **No Rayon starvation of main-thread calls in practice**: analyze and DrawKit::new during a wide Suggest are ≤2× idle and stay in milliseconds.
- **Global Rayon pool, default size (12)**: no nested custom pools and no thread explosion. One OS thread per Builder or Suggester: 37 threads with 24 plate models.

## Not checked, and why
- **UI tests**: off limits (they drive the mouse), so the Suggest-UI QoS warning could not be reproduced (C-09).
- **Swift AddressSanitizer** on the app tests: time. Rust ASan covered the unsafe code.
- **Miri**: no Miri component installed on nightly. The only unsafe code is `GpuBuffer` (ASan-clean) and the C FFI, which Miri cannot run anyway.
- **GPU memory in the running app with huge meshes** (Instruments/xctrace Metal allocations): not measured. The CPU-side sizes are in C-05 and C-07.
- **C++ under TSan**: Manifold and the OCCT shim were not sanitizer-instrumented. Manifold is built serial (`MANIFOLD_PAR=-1`), so this is low risk.
