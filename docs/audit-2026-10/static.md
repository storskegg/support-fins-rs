# Static analysis audit (area: static, IDs S-NN)

Worktree: `~/RustRoverProjects/sfrs-wt/audit-static`, detached at d2eb914 (origin/main). It is left in place, with uncommitted
experiment files only: `fuzz/` (cargo-fuzz targets, corpora, logs), `crates/sf-ffi/tests/zz_audit_probe*.rs` (probe tests),
`.swiftlint.audit.yml`, `.dd-audit/` (derived data). Nothing was committed or pushed.

## Tools used

| tool | version | what for |
|---|---|---|
| clippy | 1.99.1 (repo toolchain) | `-W clippy::pedantic -W clippy::nursery` + `unwrap_used expect_used panic indexing_slicing arithmetic_side_effects float_cmp as_conversions cast_* unreachable string_slice ...` |
| cargo-fuzz + libFuzzer (ASan) | 0.13.2, nightly | fuzzing the sf-io readers (STL, OBJ, 3MF package, 3MF model XML) |
| cargo-machete | 0.9.2 (installed) | unused dependencies |
| cargo-audit | 0.22.2 | advisories |
| SwiftLint | 0.65.1 (installed via brew) | strict config in the worktree only |
| Periphery | 3.8.0 (installed via brew) | unused Swift code (scan of the `build-for-testing` index) |
| xcodebuild build-for-testing | Xcode on the machine, under `tools/lock` | Swift compiler warnings |

Note: the user's shell exports `CXXFLAGS=-stdlib=libc++ -std=c++11`. sf-cut's build.rs already clears it, but libfuzzer-sys does
not build under it (`std::clamp` missing). Any new C++ build script needs the same guard.

---

## Findings, ranked by real-world impact

### S-01 A failed file open shows a Swift debug dump, not the engine's message
- **Severity:** High. **Confidence:** Confirmed.
- **Location:** `app/SupportFins/ViewerModel.swift:78` and `:95` (`status = error.localizedDescription`). The cause is the
  generated `sf_ffi.swift`: `FfiError: LocalizedError` with `errorDescription { String(reflecting: self) }`.
- **Evidence:** I compiled the same enum shape standalone as module `SupportFinsCore`. It prints:
  `SupportFinsCore.FfiError.Format(message: "no triangles in the file")`. `load_parts` errors reach this catch: a corrupt
  3MF, an empty file, an I/O error, "STEP import is not available", a bad OBJ index. So does `merge_parts`. Only
  `ExportModel` and `PlateBoard+Export` catch `FfiError` first and map it with `BuildModel.message`.
- **Impact:** every user whose file fails to load sees a Swift type dump in the window's status line. The web shows
  `Could not read <name>:\n<message>` (`web/ui/io.js:407`). No test opens a bad file and checks the status line:
  every `model.status` assertion in the tests checks for `nil`.
- **Recommendation (S):**
  - Catch `FfiError` in both `load` paths and use `engineMessage`, with the web's `Could not read <name>:` prefix.
  - Catch `UniffiInternalError.rustPanic` too (see S-02).
  - Add an app test that opens a 0-byte `.stl` and a junk `.3mf` and checks the text.
  - The engine strings are English-only, like the web's. Wrap the prefix in `JS.t`.

### S-02 A crafted or malformed Bambu `model_settings.config` panics the 3MF reader (fuzz-found)
- **Severity:** Medium. **Confidence:** Confirmed.
- **Location:** `crates/sf-io/src/threemf.rs:1870` and `:1890` (`parse_bambu_configs`):
  `parts[cur_id.as_ref().unwrap()].get_mut(key).unwrap()`.
- **Evidence:**
  - libFuzzer (`threemf` target, seeded from the parity corpus) hit it within about 1 minute.
  - A 600-byte reproducer is a valid 3MF whose config holds
    `<object id="1"><part id="1" subtype="modifier_part"><object id="2"><metadata key="name" value="x"/>`.
    A `<metadata key="foo">` variant hits `:1890`.
  - Through `sf_ffi::load_parts`: `panicked at crates/sf-io/src/threemf.rs:1870:38: no entry found for key`.
  - `cur_part` survives a nested `<object>` that changes `cur_id`, so the lookup indexes a key that was never inserted.
- **Impact:**
  - App: UniFFI catches the panic, because `load_parts` is a throwing call. The status line then says
    "no entry found for key", and S-01 applies to the other errors.
  - CLI: Rust panic output and exit 101 instead of an error message.
  - Not a crash, but a reachable panic on file input.
- **Recommendation (S):**
  - Keep the part's root id with the part (`(root, key)` in `cur_part_key`), or use `if let Some(..)` on both lookups.
  - Parity: in JS, `parts[curId][curPartKey]` on a missing key throws a TypeError, so the web fails this load too. Check
    what `readThreeMF` reports and either match that error or record the case in the corpus.
  - Other fuzz results: 15-minute fork runs of `threemf` and `model_xml` found nothing else.
  - The remaining `unwrap()`s in this function (lines 1851-1891) share the same invariant. Review them together.

### S-03 NaN-tolerant sort comparators can panic in Rust 1.81+ (`sort_by` "does not implement a total order")
- **Severity:** Medium. **Confidence:** the mechanism is Confirmed; reaching it through the engine is Possible.
- **Location:**
  - Six copies of `cmp_diff`: `planes.rs:228`, `orient.rs:325`, `prop/tracks.rs:26`, `prop/lattice.rs:375`,
    `prop/circular.rs:115`, `fins/coverage.rs:338`.
  - The `num_ord` sites in `sway.rs`, `fins/fill.rs` and `fins/rows.rs`.
  - `partial_cmp(..).unwrap_or(Equal)` at `footprint.rs:344` and `fins/pad.rs:631`.
  - About 30 `sort_by` sites in sf-engine in total.
- **Evidence:**
  - These comparators return `Equal` for a NaN difference, as JS's `sort` does. Rust's sort may panic when a comparator is
    not a total order.
  - A standalone probe on toolchain 1.99.1 sorted random `f64` vectors with 20% NaN using this `cmp_diff`.
    Result: 9,402 of 16,000 sorts panicked. Every length of 21 or more panicked; lengths 5-20 never did.
  - Through the FFI I did not reach it. NaN vertices, degenerate faces and a 3e38 vertex on 5 stress models
    (analyze, islands, build auto/full+sway, suggest) all sorted without a panic.
- **Impact:** if a NaN ever reaches one of these keys with 21 or more elements, the build fails. Builder and Suggester
  wrap that in `catch_unwind`, so the user sees "the engine panicked". In a non-throwing FFI call the panic would hit the
  generated Swift `try!` and crash the app. JS never throws here.
- **Recommendation (M):** use one shared `js_sort_by` helper: a stable merge sort that cannot panic and matches V8's result
  for consistent comparators. That keeps parity and the bench_m5 hashes for every finite input. Replace the six
  `cmp_diff` copies with it.

### S-04 The gate skips the app tests when only engine crates change, and nothing else enforces the gate
- **Severity:** Medium. **Confidence:** Confirmed (read `tools/gate/gate.py`).
- **Location:** `tools/gate/gate.py:46` (`APP_PATHS = ("app/", "crates/sf-ffi/")`). There is no `.github/workflows`.
- **Evidence:**
  - `sf-ffi` is a thin layer over sf-engine, sf-io, sf-cli (option parsing: `optionsFromDialog`, `compute_fins_detail`),
    sf-cut and sf-step.
  - A change in any of those changes what the app does, yet `just gate` prints "app gates skipped" and runs none of the
    ~200 app tests. Those tests are the only ones that drive the engine with the app's real `optionsJSON()` and
    `tunablesJSON()` (see S-07).
  - There is no CI, so the local gate is the only check.
  - Swift warnings are not gated: clippy is `-D warnings`, xcodebuild is not (see S-09).
- **Impact:** a change to option parsing or an export writer in sf-cli or sf-io can break the app with a green gate. This
  is the same class as the Suggest options break.
- **Recommendation (S):**
  - Count `crates/` (all of it, not just sf-ffi) and `Cargo.lock` as app-affecting in `app_changed()`. At least count
    sf-cli, sf-io and sf-engine.
  - Consider `SWIFT_TREAT_WARNINGS_AS_ERRORS` for the app target once S-09 is cleaned up.

### S-05 Footprint grids are sized by the part's bounding box with no bound: hang, a panic, or an uncatchable allocation abort
- **Severity:** Medium. **Confidence:** Likely. The abort is shown in a standalone probe; I did not crash the app.
- **Location:**
  - `crates/sf-engine/src/footprint.rs:78-82`: `vec![0u8; (w * h) as usize]` with `w, h` = the bounding box in mm.
  - Also `dilate` and `union` (`:189`, `:241`).
  - Called synchronously on the main actor from `PlateBoard.swift:215` and `PlateBoard+Scene.swift:82-83`, whenever a
    plate holds two or more models.
- **Evidence:** probe tests (release build):

  | part | footprint result |
  |---|---|
  | 2 m x 2 m | 60 ms |
  | 20 m x 20 m | 6,823 ms, a 400 MB grid |
  | a 3e38 stray vertex | panic `capacity overflow` (`footprint_of_part` is a throwing call, so Swift's `try?` gives nil) |

  For larger sizes, a standalone probe showed `vec![0u8; 1e12]` succeeding (lazily) and `vec![0u8; 1e14]` aborting:
  "memory allocation ... failed", exit 134. `catch_unwind` cannot catch that abort. A part spanning 1e7 mm in X and Y
  (one stray vertex in a broken STL, or a file in the wrong units) would therefore kill the app the moment it shares a
  plate.
- **Impact:** on multi-model plates, a part with a far stray vertex freezes the window for seconds to minutes, or crashes
  the app.
- **Recommendation (S/M):**
  - Cap `w * h` (for example at 1e8 cells) and return None or an error above it. Mirror that in the JS port notes: the
    web throws a RangeError on a typed array that size, so the web's behaviour is a failed arrange, not a crash.
  - Consider computing footprints off the main actor.

### S-06 Suggest never finishes, and cannot be cancelled, on a part with an absurd coordinate
- **Severity:** Low-Medium. **Confidence:** Confirmed.
- **Location:** `crates/sf-engine/src/prop/tracks.rs:664` (`while v <= v_hi + 1e-9 { ... v += step }`, step 0.25) inside
  `patch_tracks`. Same shape in `row_offsets`. There are 70 `while_float` loops in sf-engine.
- **Evidence:**
  - A 20 mm cube plus one stray down-facing triangle at (1e16, 1e16): analyze, build and footprint each return in 0 ms, but
    `rank_poses` was "STILL RUNNING after 60 s".
  - The staircase model with one vertex at 3e38 hung the same way: `sample` showed one thread in
    `sf_engine::prop::tracks::patch_tracks` under `orient::rank_downs` for more than 10 minutes.
  - At |v| > ~1e15, adding 0.25 no longer changes `v`. Strays at 1e4 and 1e5 were fine.
  - Cancellation is polled between candidates, so the Suggester worker stays pegged and later Suggest requests queue
    behind it.
- **Impact:** a corrupt file makes "Ranking…" spin forever at 100 % of one core. JS has the same loop, but the web can
  terminate its Worker.
- **Recommendation (S):** reject non-finite and absurd extents at load. For example, a note plus refusing to build when
  the bbox exceeds 1e6 mm, which also covers S-05. Alternatively, bound the iteration count of the float-stepping loops
  and poll `cancel` inside them. Parity holds for every sane part.

### S-07 Tests still feed `"{}"` where the app sends its real options and tunables (the class that hid the Suggest break)
- **Severity:** Medium. **Confidence:** Confirmed (counted).
- **Location:**
  - Rust: 80 `"{}"` option and tunables arguments across 9 files in `crates/sf-ffi/tests/`.
  - Swift: `SuggestModelTests` (9 of 10 `rank` calls), `ExportModelTests` (8 builds), `BuildModelTests` (10).
- **Evidence:**
  - Only `SuggestModelTests.swift:107-116`, `:164` (added by the fix), `ReportModelTests` and `DrawWallTests` use
    `FinSettings().optionsJSON()` or `tunablesJSON()`.
  - No Rust test uses the exact text the app sends. Those keys include `separateWalls: "on"`, `sway.on`, `padCustom`,
    `iface: false` and `wallLines`.
  - The Rust side does not validate tunables. Probes with `wallLines: 1e9`, `nozzle: 0`, `pinDepth: -1` and
    `tallRibsHeight: 0` all built "ok"; it relies on the Swift clamps in `FinSettings.tunablesJSON()`.
  - Dialog options, by contrast, are validated well: a bad nozzle, gap, layerHeight or tines value is rejected with the
    JS message.
- **Impact:** a parser change that rejects or misreads one of the app's real keys passes every Rust test. It also passes
  every Swift test except the few above, and, with S-04, the app tests may not run at all.
- **Recommendation (S):**
  - Check in one fixture, for example `parity/fixtures/app-request.json`, holding the default `FinSettings()`
    `optionsJSON` and `tunablesJSON` plus a sway-on/custom-pad/Draw variant.
  - Add a Swift test that asserts the app still produces exactly that text.
  - Add a Rust test that runs every JSON-taking FFI entry (`Builder.build`, `build_part`, `rank_poses`, `build_report`,
    `DrawKit`, `export*`) on it.
  - Validate the tunables' ranges in Rust (`page_tunables`) with the JS clamps.

### S-08 Accessibility identifiers on plain containers (the bug class of #204 and #206)
- **Severity:** Low. **Confidence:** Possible. UI tests not run, by the rules.
- **Location:**
  - `FinsToggle.swift:34` (`build-spinner`) and `Open/OpenFeature.swift:53` (`reading-step`): a VStack with
    `.accessibilityLabel` and `.accessibilityIdentifier`, and no `.accessibilityElement(children:)`.
  - `Toolbar/ViewToolbar.swift:69`: `toolbar-<id>` sits outside the `.overlay`, so the flash Text inherits it.
  - `CalibrationSheet.swift:21` (`calibrate-list` on a List) and `WhatsNewViews.swift:35` (`whatsnew-list` on a
    ScrollView) are probably fine, since List and ScrollView are their own elements.
- **Evidence:** a script walked every `.accessibilityIdentifier` modifier chain whose base is a `}`-closed container
  without `.accessibilityElement`. These are the six it found.
- **Impact:** a UI test that queries the spinner or a toolbar button can match the child element twice, or match the
  wrong element.
- **Recommendation (S):** add `.accessibilityElement(children: .combine)` to the two spinners, and move the toolbar
  identifier before `.overlay`.

### S-09 The Swift build has 21 unique compiler warnings, 2 of them Swift 6 isolation warnings
- **Severity:** Low. **Confidence:** Confirmed (`xcodebuild build-for-testing`, scheme SupportFins).
- **Location:**
  - 16 × `'+' was deprecated in macOS 26.0` on `Text` concatenation: WhatsNewViews, Fields, EmptyWindowView,
    ReadoutViews, StrengthPopover, SuggestPopover, CalibrationSheet.
  - `Features/Search/SearchCatalog.swift:83,133`: "main actor-isolated static property ... referenced from a Sendable
    closure".
  - `Open/OpenFlow.swift:24`: the result of `dropped(urls)` is unused. On the macOS 26 `dropDestination` overload the
    `false` that rejects an unsupported drop has no effect.
  - Tests: `FeatureSetTests.swift:53` (non-Sendable global), `ToolbarAccessibilityTests.swift:30` (deprecated API).
- **Evidence:** `swift-bindings` and `cargo build`: 0 warnings. `cargo doc`: 15 rustdoc warnings (unresolved links,
  unclosed HTML tags) in sf-io, sf-engine and sf-testkit. Neither the Swift nor the rustdoc warnings are gated.
- **Recommendation (S):** fix the two isolation warnings first: with `SWIFT_STRICT_CONCURRENCY: complete` they are
  real cross-actor reads. Then fix the drop return and the `Text +` uses.

### S-10 Crash-on-bad-value paths in Swift
- **Severity:** Low. **Confidence:** Possible.
- **Location:**
  - `Model/FinSettings.swift:212`: `precondition(v.isFinite, ...)` in `number()`, which serialises every setting.
  - `Model/ExportModel.swift:176`: `JSONSerialization.data(withJSONObject:)` on the pose and threshold. A NaN raises an
    ObjC exception, which `try?` does not catch.
  - Session form text is checked finite on the Rust side (`sf-io/src/session.rs:319`), so I found no path to a NaN.
- **Recommendation (S):** clamp or skip instead of `precondition`, and check `isFinite` before `JSONSerialization`.

### S-11 Writers silently truncate past the format limits
- **Severity:** Low. **Confidence:** Confirmed (read).
- **Location:**
  - `sf-io/src/zip.rs:110-151`, `:647-670`: u32 sizes and offsets, u16 entry counts, no Zip64.
  - `sf-io/src/stl.rs:168`: `count as u32`.
  - `sf-ffi/src/part.rs:414`: `(n_faces*3) as u32`.
- **Impact:** a 3MF over 4 GiB (tens of millions of triangles on a plate) is written corrupt without an error. JS has the
  same limit.
- **Recommendation (S):** return an error past the limit; parity is unaffected below it.

### S-12 Docs drift and dead code
- **Severity:** Info. **Confidence:** Confirmed.
- **FFI.md:** all 109 exported names are mentioned. The M4a block (`docs/FFI.md:47-51`) is stale:
  - `analyze`, `face_flags` and `islands` now return `Result<_, FfiError>`.
  - `AnalysisSummary` also has `raw_region_count` and `analyze_ms`.
- **Periphery:** 35 real "unused" results. Examples:
  - `PlateBoard+Export.swift:163 allPlateWrites()`
  - `ToolbarRegistry.swift:60 register(_:badge:)`
  - `ExportModel.swift:29 fileExtension`
  - `DrawnWall.swift:70` memberwise init
  - `ViewerRenderer.lastDrawTime`, `MoveTool.hoverPoint`
  - The Decodable-only fields of `BuildRecords.swift:136-152`
  - The 88 "assignOnly" results are Equatable keys: false positives.
- **Dependencies:** cargo-machete found no unused dependencies. cargo-audit reports one warning: `basic-toml` 0.1.10 is
  unmaintained (RUSTSEC-2026-0374, via uniffi's build tooling).
- **Duplicated helpers:** `cmp_diff` ×6, union-find `find` ×6 (i32 and usize variants), `engine()` ×4 in sf-ffi,
  `plural` ×3, `bbox`.
- **Pinning:** `sf-cut` pins `cc = "=1.8.0"` while the workspace uses `cc = "1.2"`, two cc versions. Intentional for
  Manifold's flags? Worth a comment.

### S-13 `unowned` usage (list for the concurrency auditor)
30 non-test sites. Each of these Feature files has one `unowned let host` / `model`:
FeatureSet, Calibration, Draw (feature), Export, Help, Move (feature), Open, Plates (×2), Readout, RemoveFins, Report (×2),
Rotate, Scale, Search, Section, Session, Strength, Suggest (×2), View, WhatsNew, BuildSpinner.

Plus closure captures:
- `MoveModel.swift:36-39`: `[unowned self]` ×4.
- `DrawModel.swift:204-205`: `[weak draw, unowned board]`.

---

## Clippy summary (non-test code: inline `#[cfg(test)]` modules, tests/ and parity excluded; examples included)
10,556 diagnostics outside tests (19,543 with tests) under pedantic + nursery + the restriction set. The gate's default
clippy is clean.

| lint | sf-engine | sf-io | sf-cli | sf-ffi | sf-cut | sf-step | total |
|---|---|---|---|---|---|---|---|
| unwrap_used | 52 | 50 | 17 | 19 | 1 | 0 | 139 |
| expect_used | 21 | 5 | 10 | 9 | 0 | 6 | 51 |
| panic | 2 | 9 | 7 | 1 | 0 | 4 | 23 |
| indexing_slicing | 2304 | 253 | 96 | 57 | 60 | 31 | 2801 |
| arithmetic_side_effects | 1654 | 354 | 95 | 35 | 52 | 9 | 2199 |
| as_conversions | 856 | 136 | 33 | 72 | 16 | 5 | 1118 |
| cast_possible_truncation | 236 | 44 | 6 | 32 | 7 | 1 | 326 |
| cast_sign_loss | 310 | 10 | 1 | 6 | 0 | 0 | 327 |
| cast_possible_wrap | 89 | 6 | 0 | 0 | 2 | 0 | 97 |
| float_cmp (allowed in workspace) | 57 | 8 | 7 | 1 | 1 | 0 | 74 |
| string_slice | 11 | 38 | 27 | 1 | 0 | 0 | 77 |
| while_float | 70 | 0 | 0 | 0 | 0 | 0 | 70 |

Other high-volume lints are style only: suboptimal_flops 921 (allowed on purpose), must_use_candidate 258, doc_markdown 167,
use_self 206.

**Triage.**
- **unwrap / expect / panic:**
  - sf-io: S-02 is the one fuzzing reached. The rest guard local invariants: `try_into` on fixed-size slices, the `cur`
    state of the OBJ parser (2.6M fuzz runs, no crash).
  - sf-io `report.rs` panics: input is engine-built JSON.
  - sf-cli `options.rs` / `entry.rs`: the embedded `options.json` schema.
- **i32 / u32 index casts** (union-find, CSR, weld): about 715M faces before they wrap. Not reachable.
- **`string_slice` in the CLI:** slices only after ASCII-prefix checks. Safe.
- **FFI non-throwing calls** whose panic would hit generated `try!`: only `footprint_drag_step` (it indexes `at[0..1]`
  and `to[0..1]`; Swift always passes 2). Info.

## Fuzzing totals (ASan, libFuzzer)
| target | runs | result |
|---|---|---|
| stl | 4.7M (10 min) | no crash |
| obj | 2.6M (10 min) | no crash |
| model_xml (root model + optional Bambu config, as a stored zip) | 38k (10 min) + 107k (15 min fork) | no crash |
| threemf (whole package) | crash at about 1 min (S-02), then 123k (15 min fork) | no further crash |

## What's solid (don't touch)
- **FFI argument validation is thorough:**
  - rot (9 finite numbers), threshold, offset, `arc_band`, `load_alignment` lengths and finiteness.
  - The dialog options with JS-identical range messages.
  - Export options (unknown keys rejected).
  - Session text fields checked finite.
- **Panic containment:** both workers (`sf-build`, Suggester) wrap the engine in `catch_unwind` and deliver "the engine
  panicked" through `failed`. Every file-reading FFI entry is a throwing call, so UniFFI turns a panic into a Swift
  error, not a crash.
- **Readers:** the STL and OBJ readers survived millions of fuzz inputs. The STL size check uses usize arithmetic.
- **sf-step:** without OpenCascade it fails loudly (`the_kernel_is_in_this_build` asserts it), so the gate cannot silently
  skip STEP.
- **Swift:** few force unwraps (43 outside tests, nearly all Metal object creation with guarded non-zero lengths). No
  `try!` and no `as!` outside generated code.
- **Hygiene:** no unused dependencies. Default clippy and fmt are clean. The CXXFLAGS guard in sf-cut's build.rs is
  correct.

## Could not check
- **UI tests:** never run, by the rules. S-08 is unverified for that reason.
- **App unit tests:** `just app-test` not run. Only `build-for-testing`, under the lock.
- **STEP / OpenCascade:** not fuzzed. It is C++, and a crash there is an abort rather than a panic. That would need an
  ASan C++ build of OCCT, about 30 minutes or more.
- **cargo-udeps:** not run (needs a nightly full build). cargo-machete was used instead.
- **NaN into a sort with 21 or more elements through the real engine (S-03):** not found with the inputs I tried.
- **Private fixture:** not used; nothing in this area needed it.
- **Crash dialogs:** none triggered. The allocation abort was shown only in a standalone probe binary.
