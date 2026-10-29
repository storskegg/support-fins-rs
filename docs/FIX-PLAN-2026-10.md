# Fix plan for the October 2026 audit

This is the plan for fixing what [AUDIT-2026-10.md](AUDIT-2026-10.md) found, agreed with the user on 2026-10-29. Finding IDs (C-, S-, X-) refer to the detailed files in [audit-2026-10/](audit-2026-10/).

## Decisions

| # | Decision |
|---|---|
| Scope | High and Medium findings, plus the cheap Lows that live in the same code. After this round comes a full hygiene sweep (clippy triage, dead code, duplicated helpers, warnings), and only then post-parity feature #50. |
| STEP | Import runs in a sandboxed XPC helper (OpenCascade, dynamically linked inside that helper). Only STEP goes there; the Rust readers are fixed in place. |
| Hostile input | These are always refused with a message: non-finite coordinates, component nesting deeper than 64, JSON nesting deeper than 128, and overlapping ZIP entries. These prompt with "Load anyway?" (Cancel is the default): more than 20M emitted triangles, more than 2 GB decompressed in total, or a largest dimension over 2 m. The private Earth Focus file must load without a prompt. The limits will be tuned later. |
| Divergence from JS | Allowed where the JS is itself broken on bad input, as long as the parity corpora and the 77 bench_m5 hashes stay identical. Every divergence is listed in docs/AGENTS.md under "Divergences". |
| FFI panics | Every export that takes data from a file or the user becomes `throws` and wraps its body in `catch_unwind`. Pure getters stay as they are. |
| Crash dumps | Rust: a panic hook writes the message, location, backtrace, current operation and versions. Swift: MetricKit diagnostics are written out on the next launch. Both include the last ~50 actions as breadcrumbs and the settings JSON. Reports go in `~/Library/Logs/Support Fins/` and are kept for 20 reports or 30 days. After a crash, the next launch shows a banner with Show Report / Save crash report… (the model is bundled only if the user ticks a box) / Dismiss. There is also Help ▸ Show crash reports. Nothing is sent over the network. |
| Gate | `just gate` always runs the app tests and fails on new Swift warnings. Local only; no CI. |
| Licences | Personal use for now. Add acknowledgements (OpenCascade LGPL, uniffi MPL, Manifold Apache). The user will raise distribution with the upstream owner later. |
| Staffing | Opus does T1 (window lifetimes) and the STEP XPC helper; Sonnet does everything else. Two failed reviews move a PR to Opus. The orchestrator reviews, runs the UI tests and merges. |
| Pausing | After this plan is approved, the waves run back to back with a report after each. Anything that changes how the user works with the app (refusal wording, the prompt's design) is shown in that wave's report. |

## Rules for every PR

- **Parity:** the parity corpora replay exactly, and the 77 bench_m5 output hashes are unchanged. This is checked on every engine-touching PR.
- **Tests first:** each finding gets a test that fails before the fix. Where the audit left a repro input (`sfrs-wt/audit-*/…`), a trimmed copy goes into `parity/` or `app/` test fixtures, or is generated at test time if it is big or dangerous (zip bombs).
- **No crash dialogs:** a test that proves "no crash" must run the crashing input in a child process or probe, never the test host.
- **Real JSON:** new tests use the app's real options and tunables JSON from the shared fixture (S-07), not `"{}"`.

## Wave 1: crash safety and test footing

These four tracks run in parallel. 1B starts once 1A's ViewerModel ownership design is posted, because both touch ViewerModel.

**1A. Window lifetimes (Opus).** Findings: C-01, C-02, S-13.
- Ownership model: features hold `weak` hosts, or the window owns its features and tears them down in an explicit `close()`.
- Every `Task` that outlives the window is cancelled on close and checks for the host after each `await`.
- The Suggest toolbar retain cycle is broken.
- *Accept:*
  - An app test opens and closes 10 windows with a part (small and big) and every ViewerModel, Part and builder deinitialises.
  - RSS over 5 open/close cycles stays within about 20 MB of the first.
  - No `swift_abortRetainUnowned` in a stress app test that closes windows mid-build, mid-park and mid-Draw.
  - The UI suite passes.

**1B. Readable errors.** Finding: S-01.
- `FfiError` gets `LocalizedError` with the web's wording ("Could not read X:\n<message>").
- This applies at all 5 `localizedDescription` sites.
- *Accept:* app tests assert the exact text shown for an empty file, a truncated STL and a bad 3MF, in English and one other language.

**1C. Gate and test footing.** Findings: S-04, S-07, S-09 (warnings only).
- The gate always runs app-test, and a Swift-warnings step fails on new warnings (a baseline file).
- A checked-in fixture holds the app's real options and tunables JSON. A Swift test proves `FinSettings()` still produces it, and a Rust test runs it through every JSON-taking FFI entry.
- Rust checks tunables against the Swift clamps (wallLines, nozzle, pinDepth…) and returns an error when one is out of range.
- *Accept:* `just gate` shows app-test on an engine-only change, and the fixture test fails if a key is added on one side only.

**1D. Crash dumps.** Decisions as above.
- Rust: a panic hook in sf-ffi plus an operation-context guard set by each entry point.
- Swift: a MetricKit subscriber, a breadcrumb ring buffer, the banner, Save crash report… and the Help item.
- *Accept:*
  - A forced panic in a probe produces a report with a symbolicated backtrace, the operation and breadcrumbs.
  - Retention is pruned.
  - The banner appears after a simulated crash marker.
  - No network APIs are used.

## Wave 2: hostile input

**2A. STEP in an XPC helper (Opus).** Finding: X-01 (X-12, X-13 in passing).
- An XPC service bundle runs sf-step/OCCT. The app sends the file and receives meshes or an error; a helper crash becomes "Couldn't read this STEP file".
- OpenCascade output goes to the helper's log, not stdout.
- `step_objects` indexing is checked.
- *Accept:* all 104 segfaulting mutants from the audit produce an error and the app survives (tested via the helper in a probe), and the STEP parity corpora still match.

**2B. Reader limits and crash fixes.** Findings: X-02, X-03, X-04, X-05, X-06, X-07, S-02.
- A depth cap of 64 on components (`emit_object`).
- The loader runs on a thread with an 8 MiB stack.
- An emitted-triangle counter enforces the 20M prompt limit.
- ZIP: a decompressed-size budget and rejection of overlapping central-directory records.
- JSON: a depth cap of 128 and a hashed key lookup (fixes the quadratic parse).
- Non-finite coordinates are refused, and the 2 m size check moves into analysis.
- The Bambu config unwraps become errors.
- *Accept:*
  - Every audit repro input (`component_chain_*`, `component_fanout_*`, zip bomb, overlap, `session_deep_json*`, `session_many_keys`, `coupon_one_{nan,inf,1e30}`, `crash-threemf.3mf`) gives an error or a prompt within 1 s and under 200 MB.
  - Earth Focus loads without a prompt.
  - The corpora and hashes are unchanged.

**2C. "Load anyway" prompt.** This is the Swift side of the size limits.
- Plain wording with an estimated memory figure; Cancel is the default.
- The prompt is localised in all 6 languages.
- *Accept:* app tests cover each limit, and a UI test covers the prompt and both buttons.

**2D. Engine guards.** Findings: S-05, S-06, S-03.
- Footprint grids switch to coarser cells above a cell budget (no unbounded allocation).
- `tracks.rs` loops step by a minimum that always advances, and check cancel.
- NaN-safe total-order comparators (`total_cmp`) are used wherever a float sort could see a NaN.
- *Accept:* the 1e16 vertex ranks or fails within 2 s, the 20 m part's footprint takes under 0.5 s and 50 MB, and the corpora and hashes are unchanged.

**2E. FFI throws.** Findings: X-11, C-06.
- Input-taking exports become `throws` with `catch_unwind`.
- A panicking listener callback is caught and reported (crash dump) instead of killing the worker.
- *Accept:* a fuzz-style Rust test sends hostile strings and JSON to every input-taking export with no abort, and a panicking callback test shows the next build still lands.

## Wave 3: responsiveness on big parts

**3A. Exports off the main thread.** Finding: C-03.
- STL, 3MF and plate exports run on a background task with progress and cancel. Draw wall rebuilds for plate export also move off the main thread.
- *Accept:* the main thread is never blocked for more than 50 ms during a 1.2M-face 3MF export (measured), and the export bytes are identical.

**3B. Cancel searches.** Finding: C-04.
- The Suggester gets `cancel()` and a cancel on drop, and Strength checks for staleness.
- The second stage of the ranking checks cancel inside each fin build.
- The builder cancels on drop.
- *Accept:* after closing a window mid-ranking, CPU drops within 100 ms, and a concurrent build is not slowed.

**3C. Load memory.** Findings: C-05, C-07, C-11.
- Objects are welded and given GPU meshes lazily (only the picked ones, or a preview level of detail in the picker).
- One copy of the fins per BuildResult.
- Stamp arrays are freed after use.
- *Accept:* opening Earth Focus peaks under 2 GB, a build result's memory falls by about 2/3 on a fins-heavy part, and the corpora and hashes are unchanged.

**3D. Click latency.** Finding: C-08.
- `braceAt`, `checkWall` and `sweepNow` move off the main thread with stale-result guards, or are cached.
- *Accept:* a click handler stays under 16 ms on the main thread with the 1.2M-face part.

## Wave 4: files and cleanup

**4A. Safe writes.** Findings: X-08, X-09, X-10, S-11.
- Writes are atomic (temp file plus rename), including calibration copies.
- Zip entry names are sanitised (no `/`, `..` or control characters, and names are unique).
- Writing past ZIP32 or STL limits is refused with a message.
- *Accept:* tests cover each case, and a traversal-named 3MF exports a safe zip.

**4B. Release hygiene.** Findings: X-15, S-10, S-08 (verified by the orchestrator's UI run), S-12 (FFI.md only).
- The test hooks are compiled out of release builds.
- The NaN crash paths are guarded.
- Accessibility identifiers move off containers.
- The FFI.md M4a section is refreshed.

**4C. Docs.**
- docs/AGENTS.md gets a "Divergences" section listing each one.
- An About/Acknowledgements panel credits OpenCascade (LGPL-2.1), uniffi (MPL-2.0) and Manifold (Apache-2.0).
- A `deny.toml` with the real allow-list is added to the gate.
- The `uniffi` `cli` feature moves to the bindgen binary only (X-14).

## Verification and report

- The orchestrator re-runs every audit repro and probe against main.
- Sanitizer passes are repeated: Rust TSan and ASan, Swift TSan, plus Swift ASan, which was skipped in the audit.
- The private-file stress run is repeated, and the RSS and wall-time tables are compared with the audit's.
- Full gate, UI suite, corpora and hashes.
- A report goes out with a phone notification. Then comes the hygiene sweep (scope C), and only then #50.
