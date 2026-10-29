# Security audit (area: security, IDs X-NN)

Worktree: `~/RustRoverProjects/sfrs-wt/audit-security` (detached at origin/main d2eb914, submodules initialised). Nothing was committed.

**Threat model.** A single-user, sandboxed desktop app. The realistic attacker is a model file (STL/OBJ/3MF/STEP) downloaded from a model site, opened by double-click or drag-and-drop. "Impact" below means what that file does to the user: a crash, a hang, or memory exhaustion. Severity follows that model. A crash that takes down every open window, and a whole-machine memory blow-up, rank above a slow load.

**Tools used**
- cargo-fuzz 0.13 + nightly rustc (installed for this audit) with libFuzzer/ASan.
- cargo-audit 0.22.2 (advisory DB of 2026-10-29, 1386 advisories).
- cargo-deny 0.20.2 (there is no `deny.toml`, so it ran without a config).
- Python 3 generators: `audit/gen.py`, `audit/gen_depth.py`, `audit/step_mutate.py`.
- A probe binary `audit/probe` that calls `sf_ffi::load_parts`, the same entry point as the app's `CoreLoader`. It runs on a thread with a chosen stack size: 512 KiB matches the Swift cooperative-pool thread that `Task.detached` uses in `CorePart.swift:99`. With the `build` argument it then runs analyze, islands, restore_session and build_part.
- Heavy runs went through `tools/lock/lock.py`.

**Crash reports.** The probe crashes (stack overflows, OCCT SIGSEGV) were all in the standalone CLI probe. No app or test host was crashed and no UI tests were run. These runs left about 32 `audit-probe-*.ips` files in `~/Library/Logs/DiagnosticReports/`. WATCHDOG may report them. They are expected, and they came with no dialog.

**Repro inputs:** `~/RustRoverProjects/sfrs-wt/audit-security/audit/repro/` (regenerate with `python3 -I audit/gen.py audit/repro`).

**Severity counts:** High 3, Medium 4, Low 4, Info 4.

---

## X-01 A malformed STEP file crashes the whole app (SIGSEGV inside OpenCascade)
- **Severity:** High
- **Confidence:** Confirmed
- **Location:** `crates/sf-step/src/ffi.rs:58-67` (`read_here` → `sf_step_read`) and `crates/sf-step/cpp/step_shim.cpp:255-264` (`try { … } catch (...)`).
- **Evidence:**
  - `audit/step_mutate.py` made 325 small token-level mutants of `parity/step-fixtures/*.step` (a dropped line, a `#ref` changed, a number set to an extreme value). Each was loaded through `load_parts`.
  - **104 of 325 died with exit 139 (SIGSEGV).** Every one of the 13 fixtures has crashing mutants.
  - Minimal repro: `repro/step_segv_loft.step`, which is `loft.step` with one line deleted (`#131 = VECTOR('',#132,1.);`, so a LINE now points at a missing entity).
  - Second repro: `repro/step_segv_unresolved_ref.step` (one entity renumbered).
  - Crash report: `EXC_BAD_ACCESS KERN_INVALID_ADDRESS at 0x18`, stack below.
    ```
    StepGeom_Vector::Orientation()                 <- null Handle dereferenced
    StepToGeom::MakeVectorWithMagnitude2d
    StepToGeom::MakeLine2d
    StepToGeom::MakeCurve2d
    StepToTopoDS_TranslateEdge::MakePCurve
    … STEPControl_ActorRead::TransferEntity …
    ```
  - The shim's `catch (...)` cannot catch this: it is a signal, not a C++ exception. OCCT's own signal-to-exception mapping (`OSD::SetSignal`) is not enabled.
  - Under WASM the web reads address 0x18 without faulting, so the page survives this file and the native app does not.
- **Impact:**
  - A corrupt or hostile STEP file, or a truncated download, kills the app instantly, along with every other open window and its unsaved work.
  - `load_parts` content-sniffs STEP (`part.rs:777`, `sf_step::is_step`), so a file of any extension that starts with `ISO-10303-21;` reaches OCCT.
  - OCCT 7.6.1 is a large C++ parser from 2021 that has never been hardened against hostile input. A null dereference here is a strong hint that other memory-safety bugs exist. The sandbox limits what they could reach.
- **Recommendation (M):**
  1. Best fix: run STEP import out of process, in an XPC service bundled with the app (sandbox-compatible). A crash then becomes "the CAD kernel could not read this STEP file".
  2. Cheaper fix: call `OSD::SetSignal(Standard_True)` once and wrap `Load()` in `OCC_CATCH_SIGNALS`, so SIGSEGV becomes a `Standard_Failure` that the existing `catch (...)` handles. This is partial: after a fault OCCT's state is suspect, so the next read should still be treated as unsafe.
  3. Either way, add `repro/step_segv_*.step` as regression fixtures.
  - **Parity:** none for valid files. The meshes don't change, and a file that segfaults has no native output to match.

## X-02 A deep chain of 3MF components overflows the stack and aborts the app
- **Severity:** High
- **Confidence:** Confirmed
- **Location:** `crates/sf-io/src/threemf.rs:2260-2485` (`emit_object` recurses into each `<component>`, with no depth limit).
- **Evidence:**
  - `repro/component_chain_400.3mf` is a 3 KB file: object k holds one component pointing at k-1, 400 deep.
  - Through `load_parts` on a 512 KiB thread (what the app uses) it fails with `thread '<unknown>' has overflowed its stack / fatal runtime error: stack overflow, aborting`.
  - 200 levels pass and 400 fail. With 8 MiB (CLI main thread) it fails at 20,000 levels (`component_chain_20000.3mf`, 114 KB). With 256 MiB, 20,000 passes.
  - The `seen` set only stops cycles, not depth. Stack overflow is an abort, so UniFFI's `catch_unwind` doesn't help.
- **Impact:** a 3 KB 3MF crashes the app on open, taking every window with it. In JS the same recursion throws a `RangeError` that the reader catches, so the port turns an error message into a crash.
- **Recommendation (S):**
  - Thread an explicit depth through `emit_object` and return `Err("3MF: components nested too deeply")` past a limit (e.g. 64; real files go 2-3 deep). Or convert it to an explicit stack.
  - Belt and braces: run `load_parts` from Swift on a thread with a big stack, as `sf_step::ffi::read` already does (256 MiB).
  - **Parity:** none for real files. Only the error path changes.

## X-03 Exponential 3MF component fan-out: a 1 KB file expands to billions of triangles
- **Severity:** High
- **Confidence:** Confirmed (measured up to 10^7, extrapolated beyond)
- **Location:** `threemf.rs:2444-2483`. A DAG is re-expanded at every reference: `seen` is removed again on the way out (`2484`), which is correct for instancing, but nothing bounds the total output.
- **Evidence:**
  - `repro/component_fanout_10xD.3mf`: object 1 is one triangle, and object k has ten components pointing at k-1.

    | file | size | triangles | time | peak RSS |
    |---|---|---|---|---|
    | 10x5 | 949 B | 100,000 | 38 ms | 36 MB |
    | 10x6 | 959 B | 1,000,000 | 0.37 s | 283 MB |
    | 10x7 | 969 B | 10,000,000 | 3.65 s | 2.9 GB |
    | `component_fanout_10x9.3mf` (not run) | 990 B | 10^9 | ~6 min (est.) | ~290 GB (est.) |

  - Load can't be cancelled (`CoreLoader.load`, a detached Task).
- **Impact:** a sub-1 KB "model" makes the app allocate until macOS memory pressure and jetsam kill it, or until the machine swaps hard. The user can't stop it. The same applies to many `<build><item>`s pointing at one big mesh.
- **Recommendation (S/M):**
  - Count emitted triangles in `Emit` and fail past a ceiling, e.g. 50 M triangles (the private fixture and the 7.7 M-triangle plate in the code comments sit well under that). Put the same ceiling in `read_obj` and `read_stl`'s declared count, and say so in the error text.
  - Optionally a cancel token for `load_parts`.
  - **Parity:** none below the ceiling. The bench_m5 hashes are unaffected.

## X-04 ZIP reader has no limit on decompressed size and inflates every entry, including overlapping ones
- **Severity:** Medium
- **Confidence:** Confirmed
- **Location:** `crates/sf-io/src/zip.rs:300-397` (`unzip`), `inflate_fast` (`:236`), `inflate_raw_miniz` (`:258`).
- **Evidence:**
  - `repro/zip_bomb_3gib.3mf` (3.1 MB): a valid model plus `Metadata/junk.bin` of 3 GiB of zeros. `load_parts` succeeds with **3.23 GB peak RSS**.
  - `repro/zip_overlap_4x1gib.3mf` (1.0 MB): four central-directory records that all point at the same 1 GiB deflate stream. Result: **4.30 GB RSS**.
  - The count is a u16 (or a ZIP64 u64), so 65,535 overlapping records of a 1 GB entry in about 3 MB is unbounded.
  - Entries the reader never uses (thumbnails, junk) are inflated too, because `unzip` materialises the whole archive before `find_root_part`. Overlapping or duplicate local offsets aren't rejected (project mode's `ProjectZip::new` does reject them).
- **Impact:** a few MB of 3MF can exhaust memory, as in X-03, but it needs a bigger file and the bound is per entry.
- **Recommendation (S):**
  - Check each entry's uncompressed size against the declared `uncomp` size: stop inflating past `declared + slack`, and in any case past a global budget (e.g. 4 GiB total, or 1024 × the archive size).
  - Reject entries whose `[start, start+comp)` overlaps another's.
  - Inflate lazily: only the parts the reader actually asks for (`get_part`, the config and session names).
  - **Parity:** valid files unchanged. The JS also inflates eagerly, so a lazy reader changes only resource use, not output.

## X-05 Deeply nested session JSON in a 3MF overflows the stack and aborts
- **Severity:** Medium (same crash class as X-02; the session JSON is just as reachable)
- **Confidence:** Confirmed
- **Location:** `crates/sf-io/src/json.rs` (`P::value` recurses for `[` / `{`, with no depth limit; the drop of the resulting `Json` also recurses), reached from `threemf.rs:2991` `read_session` during `load_parts`.
- **Evidence:** `repro/session_deep_json_1600.3mf` (a model plus `Metadata/support_fins.json` = `{"v":1,"x":[[[…1600…]]]}`) overflows the 512 KiB stack and aborts. 800 levels pass. `repro/session_deep_json.3mf` (200,000 levels, 1.4 KB compressed) also overflows at 8 MiB.
- **Impact:** a 1.4 KB 3MF crashes the app on open.
- **Recommendation (S):** add a depth counter to `P` (e.g. 256) that returns `Err`. `JSON.parse` in V8 handles about 10^4+ levels, but no session the app writes gets near 10.
  - **Parity:** none for real sessions. Over the limit it falls into the "session unreadable" path, which the JS also has.

## X-06 Quadratic JSON object parsing: a 460 KB session hangs the load for 33 s
- **Severity:** Medium
- **Confidence:** Confirmed
- **Location:** `json.rs:236` (`kv.iter_mut().find(|(kk, _)| *kk == k)` for every key, which is O(n²)).
- **Evidence:** `repro/session_many_keys.3mf` (a session object with 200,000 distinct keys, 463 KB) makes `load_parts` take **33.2 s** of 100% CPU (RSS 41 MB). It can't be cancelled, and the window shows a spinner.
- **Impact:** the load freezes. Doubling the keys quadruples the time, so about 1.8 MB gives about 2 minutes.
- **Recommendation (S):** keep an `IndexMap<String, usize>` (key → slot) while parsing an object, so the last value still wins in first-insertion order, as now; then `js_order`. Or cap object keys and the session size (a real session is a few KB).
  - **Parity:** output identical.

## X-07 Non-finite and extreme coordinates are accepted; one bad vertex makes a build run away (more than 32 GB)
- **Severity:** Medium (it needs the user to click Add fins on the file)
- **Confidence:** Confirmed for the resource blow-up. Possible for whether Cancel recovers it.
- **Location:**
  - The readers: `stl.rs` binary path (NaN/±inf/3e38 pass straight through; the ASCII path rejects non-finite), `threemf.rs` (`number_or0`; `NaN`/`Infinity` attributes go through).
  - The engine grid sizing. The exact allocator site was not bisected.
- **Evidence (probe `build` mode, under the lock):**
  - `repro/coupon_one_1e30.stl`: `angle-coupon.stl` (1,912 triangles) with one vertex x set to 1e30. Load, analyze and islands all succeed in under 2 ms, and the reported size is `[1e30, 39.6, 10]`. `build_part(…, "{}", "{}")` was still running after 95 s at **32.0 GB peak RSS** (killed by `timeout 90`). An earlier unlocked attempt reached 14.5 GB in 300 s.
  - `repro/coupon_scaled_1e5.stl`: the same coupon scaled to 7.6 km. The build ran over 90 s of 4-core CPU (1.9 GB) without finishing.
  - NaN or inf vertices (`coupon_one_nan.stl`, `coupon_one_inf.stl`, `stl_nan.stl`) complete quickly, and the size readout shows `inf`/`-inf`.
- **Impact:**
  - A model with one corrupt vertex (bit rot, or a broken exporter) loads normally. After Add fins the machine runs out of memory.
  - `Builder` builds are cancellable, but whether the runaway stage polls the cancel flag before allocating was not checked.
  - Separately, NaN or inf sizes reach the UI as text.
- **Recommendation (S):** sanity-check at import, outside the engine, so parity is untouched:
  - Reject non-finite coordinates in the binary STL and 3MF paths, as ASCII STL already does.
  - Reject a part whose bounding box exceeds a sane printer limit, e.g. 10 m, or warn and offer to rescale.
  - Whether the JS behaves the same is unverified. It probably throws `RangeError: Invalid array length` and survives.

## X-08 "All plates" zip entry names come from untrusted 3MF/OBJ/STEP object names, path traversal included
- **Severity:** Low
- **Confidence:** Confirmed (the name flows through; the zip writer stores it verbatim)
- **Location:**
  - `app/SupportFins/Plates/PlateBoard+Export.swift:50-55,150`: the name is `"plate-N-" + exportFileName(base: part.info.name…)`.
  - `crates/sf-ffi/src/part.rs:796-801`: the part name is the file's object name.
  - `crates/sf-io/src/zip.rs:48-50`: "stored verbatim".
- **Evidence:** `repro/3mf_traversal_name.3mf` has the object name `x/../../../../Library/LaunchAgents/evil`. `load_parts` returns exactly that name, so the zip entry becomes `plate-1-x/../../../../Library/LaunchAgents/evil-and-1-more-fins-0.4nozzle-0.2layer.3mf`.
- **Impact:**
  - When the user later extracts the .zip, an extractor that doesn't sanitise `..` writes outside the folder. Archive Utility and `ditto` do sanitise; some third-party tools don't.
  - The content is a 3MF with the extension forced to .3mf, so it can't plant an executable. Names over 65,535 bytes also corrupt the archive (X-10).
  - The same name pre-fills NSSavePanel, which is harmless.
- **Recommendation (S):** sanitise the base name in Swift `ExportModel.base(of:)`, or in `export_file_name`: map `/ \ :` and control characters to `-`, collapse `..`, and trim to about 100 characters. `export_file_name` has parity tests against the JS `plateName`, so either sanitise in Swift only or add the JS's own sanitising if it has one.

## X-09 Exports and calibration downloads are not atomic: a failed write destroys the old file
- **Severity:** Low
- **Confidence:** Confirmed (by reading)
- **Location:**
  - `crates/sf-ffi/src/builder.rs:757-761` (`write_file` = `std::fs::write`, which truncates and then writes).
  - `app/SupportFins/Features/Calibration/CalibrationModel.swift:32-33` (`removeItem` and then `copyItem`).
  - CLI `sf-cli/src/main.rs:15` (plain export via `std::fs::write`).
- **Impact:** when overwriting an existing export, a disk-full error or a crash mid-write leaves a truncated file, or none at all for the calibration path. The CLI's project mode (`project_io.rs`) already does this properly: temp file, fsync, rename.
- **Recommendation (S):** write to a temp file beside the target, then rename: Swift `Data.write(options: .atomic)`, or `FileManager.replaceItemAt`, or reuse `project_io::write_project`'s pattern. No parity impact.

## X-10 `zip_store` silently wraps fields past the ZIP32 limits
- **Severity:** Low
- **Confidence:** Confirmed (by reading)
- **Location:** `zip.rs:104-155`. The writes are `as u16` for the name length and entry count, and `as u32` for sizes and offsets.
- **Impact:**
  - A name over 64 KiB, more than 65,535 entries, or a plate or zip over 4 GiB produces a corrupt archive without an error. Only All plates of very large plates could reach the 4 GiB limit.
  - The project-mode `patch` already checks with "output would require ZIP64".
- **Recommendation (S):** return an `Err` from `zip_store` (or `export_plates`) when any limit is exceeded. The JS has the same limits, so the output is unchanged wherever the JS also produces a valid archive.

## X-11 UniFFI panics in non-throwing exports become `try!` crashes in Swift
- **Severity:** Low (fuzzing found no panic on the reachable path)
- **Confidence:** Confirmed (the mechanism). The trigger is only Possible.
- **Location:**
  - The generated `sf_ffi.swift` has 58 `try! rustCall(` sites and 26 throwing ones. A Rust panic in a non-throwing export reaches `try!`, which is a Swift trap.
  - Example of untrusted input on a non-throwing path: `Part::restore_session` (`crates/sf-ffi/src/session.rs:61`) restores the JSON from the downloaded 3MF.
- **Evidence:**
  - `fuzz/json_session` ran `restore_session`, `fins_opts_from_json`, `tunables_from_json` and `model_from_json` on arbitrary JSON for 10.15 M executions (15 min) with no panic.
  - Throwing exports such as `load_parts` convert a panic into `UniffiInternalError.rustPanic`, which is fine.
  - The release profile keeps `panic = "unwind"`.
- **Recommendation (S):** make the exports that touch file-derived data throwing (`restore_session` especially), or wrap their bodies in `catch_unwind` with a safe default. This is the same class of gap as the seed note about FFI tests passing `"{}"`: add an FFI test that feeds the session corpus (`parity/corpus/session/*.3mf`) through `restore_session`.

## X-12 (Info) `step_objects` indexes `nodes[0]` and children without bounds checks
- **Location:** `crates/sf-step/src/lib.rs` (`step_objects`, the `result.nodes[node]` loop; `subtree_meshes`).
- A success result with zero nodes, or a child or mesh index out of range, would panic. That panic would be caught (`load_parts` throws), and the shim always emits a root, so this isn't reachable today. An `Option`-returning lookup would make it robust. Not reproduced.

## X-13 (Info) OpenCascade writes its diagnostics to the app's stdout
- The OCCT reader prints coloured `*** ERR StepReaderData : Unresolved Reference …` lines to stdout and stderr in-process. They aren't user paths or secrets, just noise in Console and logs.
- `Message::DefaultMessenger()->RemovePrinters(...)` in the shim would silence them.

## X-14 (Info) Dependencies and licences
- **cargo-audit:** no vulnerabilities.
  - One warning: RUSTSEC-2026-0374, `basic-toml 0.1.10` is unmaintained. It enters via `uniffi_bindgen → askama_derive` (a proc-macro, build-time only), pulled in by `uniffi` with `features = ["cli"]` in `sf-ffi`'s normal deps. The `cli` feature could move to the `uniffi-bindgen` bin only, but the impact is negligible either way.
- **cargo-deny:** no `deny.toml`, so every crate fails the empty licence allow-list (102 "rejected", which is noise). One duplicate: `syn` 1 and 2.
- **Licence notes worth recording:**
  - The `uniffi*` crates are MPL-2.0 (file-level copyleft; fine for a binary, but they need a notice).
  - OpenCascade (LGPL-2.1 + OCCT exception) is **statically linked** into the app. LGPL-2.1 §6 then expects a way to relink, e.g. shipping object files or linking OCCT dynamically. The sf-step README already flags an About/acknowledgements task for M7.
  - Manifold (sf-cut) is Apache-2.0.
  - Recommendation: add a `deny.toml` with the actual allow-list so the gate catches new licences and advisories (S).
- **SwiftPM:** no remote packages (`app/*/Package.swift` has no `.package(url:)`).
- **Deno tooling (dev-only):** `tools/i18n` and `tools/tippics` pin `npm:acorn@8.14.0`. `tools/helpbook` pins `npm:linkedom@0.18.5`. Both have lockfiles and `nodeModulesDir: none`. The dev dashboard (`tools/dashboard/server.py`) binds 127.0.0.1, serves only GET routes, and has no CORS header. Low risk.

## X-15 (Info) Release builds honour the `SFChangelogFixture` default
- **Location:** `app/SupportFins/Features/WhatsNew/Changelog.swift:38`.
- A UI-test hook that reads What's New from any path named in UserDefaults or the launch arguments. Under the sandbox it can only read files the app may read, and it is decoded with `JSONDecoder`, so the impact is cosmetic.
- Gate it behind `#if DEBUG` or a test-host check (S).

---

## Fuzzing summary (cargo-fuzz, libFuzzer + ASan)
All six targets ran **concurrently for 15 min each under the lock**, with `-rss_limit_mb=6144 -timeout=20 -max_len=1MiB`.
- Seeds: the calibration coupons, the `parity/corpus/session` 3MFs, small Manifold OBJ test models, and the extracted model XML and session JSON.
- Harnesses: `audit/fuzz/fuzz_targets/*.rs`. Re-run with `CXXFLAGS=-std=c++17 cargo +nightly fuzz build -O --fuzz-dir .`, then `./run.sh <secs>`. The user's shell `CXXFLAGS` forces `-std=c++11`, which breaks the libFuzzer build.

| target | what it drives | execs | cov | result |
|---|---|---|---|---|
| stl | `read_stl` | 4.68 M | 540 | clean |
| obj | `read_obj`, `read_obj_chunked(4)` (asserts object count matches), `read_obj_reference` | 0.77 M | 1933 | clean |
| threemf | `read_three_mf` on whole packages | 61 k (slow: MB-sized seeds) | 3197 | clean |
| threemf_xml | model XML wrapped by `zip_store` → `read_three_mf` and strict `read_three_mf_parts{with_components, build_index:0}` | 76 k | 2742 | clean |
| json_session | `json::parse`, `stringify`, `restore_session`, `fins_opts_from_json`, `tunables_from_json`, `model_from_json` | 10.15 M | 1416 | clean |
| project_zip | `ProjectZip::new/read/patch`, `unzip` | 4.64 M | 1505 | clean |

No crashes, OOMs or slow units (slowest unit under 1 s). The resource bugs above (X-02 to X-06) are structural: random mutation won't find them within 1 MiB, so I found them by reading the code and building targeted inputs. STEP (OCCT) wasn't fuzzed with libFuzzer, because it would need an instrumented OCCT build. The 325 structured mutants of X-01 stood in for that.

## What's solid (don't touch)
- **App sandbox:** App Sandbox and Hardened Runtime are on. The only entitlement is `files.user-selected.read-write`. There is no network entitlement, so the app itself can't fetch anything. The calibration "downloads" are bundled files copied out through NSSavePanel. Recents go through `NSDocumentController.noteNewRecentDocumentURL`, so access to recent files after relaunch is handled by the system.
- **URLs:** every URL the app opens is a hard-coded `https://` constant (GitHub, ko-fi, the calibration guide, releases). The report-a-problem link (`report_url`) is built from fixed hosts, and every field is `form_encode`d. It carries only settings, counts, part size and triangle count, the engine version and the date: no file name or path. It opens in the browser, so nothing is sent without the user.
- **No dynamic code:** no `Process`/`NSTask`, `dlopen`, `URLSession` or WKWebView. The Help Book has no `<script>` and no remote resources.
- **No path logging:** no `os_log`/`Logger`/`print` of user paths in the app. Paths appear only in error messages shown to the user.
- **ZIP reader:** every fixed-field read is bounds-checked (`View::get` uses `checked_add`). Entries are never extracted to disk, `p:path` resolves only inside the package, and ZIP64 placeholders without a record are rejected.
- **XML scanner** (`xmlscan.rs`): hand-rolled, with no DTD, entity expansion or external references, so billion-laughs and XXE don't apply. Comments, CDATA and PIs are skipped by searching for their terminator, which is linear.
- **Triangle indices:** out of range, negative or NaN indices drop the face (`threemf.rs:2364-2371`). Fuzzing confirmed there are no OOB panics.
- **STL:** the binary count is checked against the length before reading, and ASCII STL rejects non-finite values.
- **UniFFI panics:** a panic in throwing exports (including `load_parts`) becomes a Swift error. `Builder` builds `catch_unwind` on their thread.
- **CLI project mode:** atomic temp file, fsync and rename. It refuses symlinked or hard-linked outputs and won't overwrite the input. Cache files are created with `create_new` and mode 0600, and the cache dir is refused if it's a symlink.

## Not checked, and why
- Whether `Builder.cancel` stops the X-07 runaway before it allocates: that would need a cancel-timing harness. Left as Possible.
- Whether upstream JS shows the same X-03, X-06 and X-07 resource behaviour: no browser run.
- libFuzzer on OCCT: it needs an instrumented OpenCascade rebuild (about 30 min). The structured mutants already found a crash.
- The private fixture: not needed for any finding, so not loaded.
