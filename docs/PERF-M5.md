# M5 performance

Final M5 numbers for the Rust port against the web engine (JS) and the single-threaded Rust engine of M3 ([PERF-M3.md](PERF-M3.md)). Targets are from [PORT-PLAN.md](PORT-PLAN.md) section 1.

## Summary

- The parallel engine builds fins 1.9-2.0x faster than M3 serial on the stress set (3.6-3.7x faster than JS), and 2.75x faster on the 772k-triangle private objects (JS cannot open that file).
- Loading and welding a 1.5M-face STL takes about 55 ms (was about 535 ms); the 7.7M-triangle private 3MF parses in 1.0 s (was 8.4 s).
- Suggest searches up to 10x more poses on curved and many-faceted parts in 2.3-71x less wall time than the web.
- The renderer is not the limit: a 1.5M-triangle scene with 9000 fins and tines costs 1.0 ms of GPU per frame (8.33 ms budget).
- All five targets are met on what could be measured. Open items: a 772k-triangle part during a live drag rebuilds in about 390 ms (fins trail the ring by about 10 degrees), `flat_bracket.stl` itself was not available, and the private-file Suggest row was not re-recorded.

## Targets

| target | measured | status |
|---|---|---|
| 120 fps at 1M+ triangles | 1.5M triangles, 3000 fins + 6000 tines, 2560x1600: GPU 1.02 ms mean, 1.07 ms p99, ceiling 1570 fps (orbit). Earlier measurements: 7.7M triangles 4.0 ms GPU; live app at 120 Hz on 1.2M triangles 119.9-120.0 fps | Met: the GPU uses an eighth of the 8.33 ms frame. The remaining risk is main-thread time (SwiftUI): about 1% of live-app frames slip a display tick |
| Live Auto rebuild under ~50 ms on a typical part during a drag | One build on the app's path (re-weld, analyse, fins) under the 8 ms supersede policy: lowledge 6.0, coin 13.1, cone 17.0, needle 20.1, torus 22.2 ms; fins lag the ring by 0.3-1.5 degrees median | Met on the stress proxies. Not met on the 772k-triangle private object: 393 ms (about 120 ms of it the per-pose weld), fins trail by about 10 degrees |
| `flat_bracket.stl` under 150 ms | Not available in this repo; worst proxy instead. Slowest now: tube X60Z30 (384 triangles) 32 ms (M3 serial 52.7, JS 88.2). Slowest in JS: cone X40 18.5 ms (M3 61.2, JS 124). Largest case: private object 336 ms median (M3 serial 935) | Met on proxies, unverified on the file: no proxy has flat_bracket's 2.4 s JS time, so this shows the stress set is far under 150 ms, not that flat_bracket is |
| Suggest at 10x the candidates in less wall time than the web | Sphere 18 to 180 candidates, torus 14 to 156, needle 17 to 150, coupons 12 to 95-162; JS / wide wall time 3.9x, 3.9x, 5.0x, 5.6-71x. Cone, coin, cylinder, tube gain 3-4x the candidates (54-70), still 3.9-10x faster. Boxes, L-brackets, plus shapes have no more stable down-faces (6-10 stay 6-10) | Met for curved and many-faceted parts, faster than the web on 25 of 29 inputs; cube, plate and pyramid (12 or 8 triangles) take 0.1 ms against the web's 0.05-0.07 ms (pool start-up) |
| 1.5M-face load and weld well under a second | Binary STL: read 8 + parse 7.5 + weld 37 ms, first analyze 12 ms. OBJ: parse 31 + weld 37 ms | Met: about 55 ms to a welded topology (was about 535 ms) |

## Fin build

`just bench-m5` (`bench_m5`): `build_fins` per case with the site's own request (Auto, PLA, tines, bed pad, threshold 45), median of 5 after a warm-up (3 for the private objects), the better of two runs per case. The JS and M3 serial columns are PERF-M3.md's. Builds run on the global Rayon pool (12 threads; a build parallelises inside a wall, since most stress parts have only 1-3 patches per pass).

| set | builds | JS ms | M3 serial ms | now ms | now vs M3 serial | now vs JS | median build now |
|---|---:|---:|---:|---:|---:|---:|---:|
| Stress models at golden poses | 48 | 1212 | 613 | 327 | 1.87x | 3.7x | 4.5 ms |
| Worst orientations (torus, sphere, needle, tube, coin) | 29 | 1300 | 714 | 361 | 1.98x | 3.6x | 11.6 ms |
| Private objects (10 x 772,340 triangles) | 10 | cannot load (V8 string limit) | 9367 | 3403 | 2.75x | - | 336 ms |

Builds under 5 ms gain nothing from parallelism (setup dominates). Across the 54 builds that take 5 ms or more in M3, the speed-up over M3 serial is 1.04x-4.4x, median 1.8x.

The slow proxies:

| case | triangles | JS ms | M3 serial ms | now ms | now vs M3 serial | now vs JS |
|---|---:|---:|---:|---:|---:|---:|
| tube X60Z30 | 384 | 88.2 | 52.7 | 32.1 | 1.6x | 2.7x |
| torus X30 | 2048 | 59.8 | 34.9 | 24.5 | 1.4x | 2.4x |
| lowledge up | 20 | 82.7 | 36.7 | 23.4 | 1.6x | 3.5x |
| torus X45 | 2048 | 98.8 | 51.1 | 22.7 | 2.3x | 4.4x |
| needle X30 | 384 | 97.9 | 51.6 | 19.0 | 2.7x | 5.2x |
| cone X40 | 96 | 124 | 61.2 | 18.5 | 3.3x | 6.7x |
| needle X40 | 384 | 94.4 | 48.7 | 18.3 | 2.7x | 5.2x |
| ushape X40 | 28 | 80.1 | 41.3 | 18.2 | 2.3x | 4.4x |
| needle X60Z30 | 384 | 119 | 68.0 | 18.0 | 3.8x | 6.6x |
| needle X60 | 384 | 119 | 65.4 | 14.9 | 4.4x | 8.0x |
| coin X40 | 256 | 72.0 | 37.1 | 12.4 | 3.0x | 5.8x |

The 12 slowest builds together: JS 980 ms, M3 serial 520 ms, now 267 ms.

Where the build time went (macOS `sample` plus stage timers, serial, before M5), in the order it was acted on: `bare_area` re-walked every red triangle twice (25-57% of needle and coin); Auto's judge rebuilt the same base twice when a region swapped to a downhill wall (a third of cone, torus, ushape); the finished-wall weld check and per-station probes were about 70% of a cone build; the bed pad's brim lattice was half of ushape X40; on the 772k-triangle objects half the build was `automatic_targets` (a floor probe under each excluded tiny-hole face, one surface grid per pass). Left unchanged: the per-wall corridor early-out (`inside_part` is a ray-parity test, so an empty box does not imply a clear one) and merging `station_is_clear` with `station_certified` (the second runs after `settle_top` moves the line).

`just bench-m5` writes the full 87-case tables between the markers below; the figures above are the better of two such runs.

<!-- bench:begin -->
<!-- bench:end -->

## Load and weld

`just bench-load` (`bench_load`): read = `fs::read` (page-cached), parse = bytes to a position soup, weld = `build_topology`, analyze = the first `analyze` (identity pose, 45 degrees). Median of 5 after a warm-up (private file: parse median of 3; weld and analyze are the mean over its 10 objects). The 1.5M-face inputs are a generated torus under `target/bench-load/`, never committed. Better of two runs.

| input | triangles | read ms | parse ms | weld ms | analyze ms |
|---|---:|---:|---:|---:|---:|
| binary STL | 1,500,000 | 8.2 | 7.5 | 37.2 | 12.4 |
| OBJ | 1,500,000 | 5.7 | 31.3 | 36.8 | 12.2 |
| private 3MF (whole file; weld and analyze per object) | 7,723,400 | - | 1019 | 20.8 | 6.4 |

Before (M3 `main` at cc3b2ad, loaded machine): STL weld 478-522 ms, analyze 23 ms; OBJ parse 434 ms, weld 499 ms; private 3MF parse 8300-8500 ms, weld 217 ms and analyze 13 ms per object.

What changed: the weld groups equal corners by hash bucket on all cores and recovers the serial id order from each group's first corner (480 to 40 ms); the first analyze runs its rotation and per-face passes on all cores (23 to 12 ms); the OBJ reader scans line chunks in parallel (434 to 31 ms); the 3MF model XML is a zero-allocation scanner over pieces cut at element starts, scanned in parallel (8.4 s to 1.0 s; what is left is the 0.75 s inflate of one 633 MB deflate stream, serial with zlib-rs). Topology digests are unchanged (see Method). Loading is one blocking FFI call and is not cancellable.

## Suggest

`just bench-suggest` times `suggestOrientations` (top 3, threshold 45, default config) in the web (Deno at the pin) and in Rust on the same files, median of 5 after a warm-up. Serial = one-thread pool, parallel = global pool at the web's candidate set, wide = `suggest_orientations_wide` at 10x density. One run (see Method); selected rows:

| model | triangles | web cands | JS ms | Rust serial ms | Rust parallel ms | wide cands | wide ms | JS / wide |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| sphere | 1280 | 18 | 51.5 | 26.6 | 8.73 | 180 | 8.07 | 6.4x |
| torus | 2048 | 14 | 92.8 | 46.8 | 18.1 | 180 | 23.6 | 3.9x |
| needle | 384 | 17 | 142 | 60.7 | 12.5 | 150 | 28.5 | 5.0x |
| cone | 96 | 17 | 46.0 | 16.4 | 4.61 | 54 | 4.55 | 10.1x |
| coin | 256 | 16 | 97.7 | 40.9 | 12.1 | 70 | 11.6 | 8.4x |
| torus_flat | 6144 | 14 | 146 | 69.7 | 22.4 | 156 | 27.6 | 5.3x |
| bore-coupon | 9448 | 12 | 826 | 419 | 129 | 139 | 146 | 5.6x |
| orient-coupon | 9614 | 12 | 3231 | 1224 | 473 | 108 | 221 | 14.6x |
| honeycomb-coupon | 18460 | 12 | 954 | 459 | 174 | 109 | 61.6 | 15.5x |
| cutout-coupon | 16766 | 12 | 1389 | 640 | 286 | 95 | 71.8 | 19.3x |
| tall-coupon | 17498 | 12 | 9442 | 4343 | 1128 | 115 | 1000 | 9.4x |
| sampler-coupon | 15538 | 12 | 17091 | 5969 | 2109 | 162 | 241 | 71.0x |

Over all 29 inputs (22 stress models, 6 coupons, torus_flat) the wide search is faster than the web on 25 (22 at 2.3x or more); cube, plate and pyramid are 0.1 ms slower in absolute terms and ramp ties. Boxes, L-brackets and plus shapes keep their 6-10 candidates.

Why wide costs about what the web search costs: the ranking is over 99.9% finalist `build_fins` (5 finalists), and scoring a candidate is cheap (needle: 0.1 ms for 17 candidates, 0.9 ms for 150). The parallel speed-up is bounded by the slowest finalist build (3-4x on 12 cores for the large coupons). On the large coupons wide beats the web-set parallel run because the wider pass 1 picks cheaper finalists; the scores are the web's.

The private object (one object, 772,340 triangles) was timed earlier on a loaded machine: 14 web candidates, 3626 ms serial, 1428 ms parallel; 134 wide candidates in 1340 ms. It is not re-recorded: run 2 hung before reaching it (Known issues) and run 1 printed no row for it.

## Rendering

`just bench-render` runs `viewer-bench`, a headless release build: a generated lumpy sphere of 1.5M triangles (flat-shaded, overhang flags) with 3000 thin walls and 6000 eight-sided tines under it, drawn through the real colour pass at 2560x1600, three frames in flight, 600 frames per scenario. `fps` is the machine's ceiling (nothing waits for a display). Clean run (render-1; the second run started at load 5.9 and is not used):

| scenario | CPU mean ms | GPU mean ms | GPU p99 ms | frame interval p99 ms | fps ceiling |
|---|---:|---:|---:|---:|---:|
| orbit | 0.012 | 1.017 | 1.071 | 0.691 | 1572 |
| panzoom | 0.012 | 0.948 | 1.031 | 0.723 | 1602 |
| drag (flags buffer every 4 frames) | 0.045 | 1.026 | 1.140 | 0.819 | 1569 |
| swap (new fins every 32 frames) | 0.016 | 1.023 | 1.066 | 0.799 | 1571 |
| liveDrag (new build every 4 frames, 30 a second) | 0.047-0.049 | 1.024-1.028 | 1.072-1.183 | 0.830-0.837 | 1567-1574 |

A face pick (ID pass plus one pixel read-back) costs 0.94 ms. It runs on mouse move only, never during an orbit or a ring drag.

Earlier measurements on a busy machine, not repeated: GPU time scales with triangles (990k 1.26 ms, 2.0M 1.4 ms, 4.0M 2.4 ms, 7.7M 4.0 ms); the live app on a 1.2M-triangle torus holds 119.9-120.0 fps with a frame-interval p99 of 9-13 ms (about 1% of frames slip a display tick). The Time Profiler showed the main thread, not the GPU, was the cost: 40% busy while orbiting, nearly all of it SwiftUI redrawing the view cube per camera change. Labels as Canvas symbols, asynchronous Canvases and the Home button split out of the view that reads `cameraRevision` removed about 10% of a core with identical pixels; fps is unchanged.

Overhang shading during a ring drag runs one analysis at a time off the main actor (newest pose wins), so colours trail the turn by one analysis and never block a frame. A drag asks for flags only (`wantsSummary: false`), which halves the engine time per shade: 20.5 ms at 1.2M triangles, 13.2 ms for one private object (about 50 and 75 refreshes a second).

## Live drag and cancellation

Builds are cancellable (`build_fins_cancellable`; the `Builder` raises the flag of the build in flight when a newer one arrives). `bench_cancel` builds each case on a warm topology and raises the flag at evenly spread fractions of the build (16 trials, 8 on the private objects); latency is flag store to return. Better of two clean runs, 87 cases: 1104 stopped builds, median 0.12 ms, p95 2.2 ms, p99 5.5 ms, max 11.6 ms (the stress models stay under 6 ms; the maximum is a private object, whose parallel passes over every face poll only between passes). The polls cost 0.9% of the 87-build total.

The app rebuilds during a ring drag (View > Rebuild While Dragging, on by default; off is the web's rebuild-on-release). A new pose starts a build at once; at most one build is in flight; a new pose supersedes it only while it is younger than 8 ms (`BuildController.liveLandAfter`), otherwise the build lands and the newest pose is built next. `bench_drag` simulates 60 Hz pose events over 1.5 s through the real build path (`build_part`: re-weld, analyse, build); lag is the angle between ring and fins on screen. Clean run (drag-1; the second started at load 10.6 and is not used):

| model | one build ms | always supersede: landed, median / max lag deg | 8 ms: landed, median / max lag deg | never supersede: landed, median / max lag deg, last pose to final fins ms |
|---|---:|---|---|---|
| lowledge X30 | 6.0 | 60/90, 0.3 / 10.1 | 77/90, 0.3 / 1.0 | 78/78, 0.3 / 1.3, 6.4 |
| coin X40 | 13.1 | 55/90, 0.4 / 16.2 | 74/77, 0.4 / 4.5 | 74/74, 0.4 / 4.5, 14.1 |
| cone X40 | 17.0 | 3/90, 19.8 / 39.6 | 67/81, 0.9 / 2.2 | 70/70, 0.9 / 2.2, 20.2 |
| needle X60 | 20.1 | 16/90, 14.8 / 44.5 | 66/84, 1.3 / 4.7 | 73/73, 1.3 / 4.0, 39.2 |
| torus X45 | 22.2 | 1/90, 22.8 / 45.0 | 46/82, 1.5 / 2.5 | 53/53, 1.5 / 2.5, 25.2 |
| private object, X30 | 393 | 1/90, 15.2 / 30.0 | 5/5, 10.4 / 16.9 | 5/5, 10.1 / 15.8, 411 |

8 ms keeps the fins within 1-5 degrees of the ring on every stress proxy and the release no later than one build. On the private object a pose costs 393 ms (about 120 ms is the weld of the posed copy), so the fins trail by about 10 degrees and the release waits one build. Landing a build on the main actor is 0.2 ms.

Frame time while builds land is the `liveDrag` row above. A landed build is a new immutable buffer pair (the Rust allocation wrapped without a copy, freed after the command buffers using it complete), so the swap needs no double buffering; Metal API validation at 100 swaps a second reported no errors.

## Memory

Peak of `bench_load 3mf` (`/usr/bin/time -l`): the whole private 3MF read, parsed to a soup, and each of its 10 objects (7.7M triangles) welded and analysed: maximum resident set 6.85 GB (6.4 GiB), peak footprint 6.86 GB, 4.8 s wall. M3's `bench_m3` process (the same file plus the stress set) peaked at 6034 MiB. `read_three_mf_unmerged` alone (the app's call) fell from 3.39 GB to 1.95 GB when the 3MF scan was rewritten. The stress models add a few MB; the one file dominates.

## Method

- **Machine:** Apple M2 Max, 12 cores, macOS 26.6.2, rustc 1.99.1, `cargo build --release`; Swift release build for the viewer; Deno 2.9.8 at the pin (299ef49) for JS.
- **Two runs, better median:** each benchmark ran twice, 20-35 minutes apart, and each cell keeps the lower median. Exceptions: rendering and live drag use the run that started clean (render-1, drag-1), and suggest has one run (below). A second run's tables are not shown.
- **Baseline-relative load:** this Mac's idle floor is about 2.7-3.5 on the 1-minute load average (WindowServer and the Claude app; measured as 3.45 and 2.73 with none of our work running). A step is clean when it started within about 1.5 of that. Load before each step:

| step | run 1 | run 2 |
|---|---:|---:|
| fin build | 1.57 clean | 1.90 clean |
| load | 3.83 clean | 3.18 clean |
| suggest | 2.76 clean | not used (hung) |
| cancel | 1.97 clean | 1.78 clean |
| render | 1.90 clean | 5.89 flagged: GPU 1.10-1.20 ms instead of 1.02; not used |
| drag | 1.88 clean | 10.57 flagged: private build 430-484 ms instead of 393; not used |
| memory | 2.71 clean | - |

- **Suggest:** run 1 alone. Run 2 hung on an engine deadlock (Known issues) and was abandoned.
- **Output identity:** the 87 fin-build output hashes are identical between the two runs, and the cancellation polls changed none of them (all 87 matched main's when they were added). The topology digest of the 1.5M-face inputs is identical for STL and OBJ (b319f936ecbf89cd) and for the private file (b97ec1394e5c3e3f) in both runs, and unchanged by the load work. `bench_suggest` asserts in-process that the parallel ranking equals the one-thread ranking and that the web's candidate count equals Rust's; the default path is byte-identical to the parity corpus (`parity_orient`). The "Engine threads: 1" in the run files' header was a stale constant; builds ran on the 12-thread pool, and `bench_m5` now reports it.
- **Private file:** `private-fixtures/Earth Focus Many Vertical.3mf` is git-ignored; only its name, counts and timings appear here. One run-1 outlier (object 10, 2086 ms against 336 for the others, same output hash) did not recur and is dropped by the better-of rule.

## What didn't pay off

- Load: a larger streaming inflate buffer (64 KB to 4 MB) changed nothing; overlapping the inflate with the XML scan caps at about 10% of the load for a reader that must carry elements across buffers (not built); libdeflate inflates 1.7x faster than zlib-rs but cannot report the bytes a stream used without `unsafe` FFI (not done); parallel `emit_object` shares the painter and role arena for about 13 ms per object (not done).
- Rendering: instancing tines, indirect draws, triple-buffered uniforms, picking on demand and double-buffering the fins have nothing to remove at 1 ms of GPU and 0.03 ms of encode per frame; an incrementally updated residency set gave identical medians (interval p99 1.96 against 2.01 ms) and was not kept.
- Live drag: "a new pose always cancels the build in flight" starves (1 build in 90 at 22 ms); "never cancel" leaves the final fins up to two builds late (needle: 39 ms against 22); a 25 ms or 100 ms threshold behaves as "always" for these builds.
- Cancellation: an unwinding poll inside a Rayon closure made a 13 ms build take 50 ms to stop (every job unwound separately). Inside a parallel loop a raised flag now makes the remaining items return a placeholder and one poll after the loop unwinds once (0.09 ms median).
- Fin build: builds under 5 ms do not speed up, and a part with one patch per pass cannot be parallelised across walls, only inside one; that is why the stress set gains 1.9x rather than several times that on 12 cores.
- Suggest: only `build_fins` can lower the wall time of the large coupons; pass 1 is already negligible.

### Known issues

- Parallel lazy grid builds (#154) could deadlock under nested Rayon work (found in the quiet run; fixed in a follow-up PR).

## Sway click on a million-face part (upstream catch-up U2)

`just bench-sway` (`bench_sway`): `sway_at_face` on a synthetic 1.6M-triangle tower (30 x 20 x 80 mm, turned 30 degrees about Z, 0.1 mm facets, a ledge near the top so the rib's shrink loop runs), 24 clicks spread over the upright faces with cross ribs on. Ports of upstream's `triindex.js` (XY triangle grid, built once per part), `swayhit.js` (candidate triangles checked against the rib's (s, u, z) box before clipping) and `swaycrop.js` (cross ribs on a crop of the part, not a sheared copy). Machine load was 20-25 during both runs, so read the ratio, not the milliseconds.

| | first click (builds the grid) | median of the other clicks | slowest | output hash |
|---|---:|---:|---:|---|
| before (main) | 264 ms | 752 ms | 2504 ms | a9885958e2606b45 |
| after | 264-269 ms | 161-167 ms | 490-527 ms | a9885958e2606b45 |
