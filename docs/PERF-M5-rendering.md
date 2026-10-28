# M5 rendering: frame time on a 1M+ triangle part

Track A of M5 (target: orbit, pan and zoom a 1M+ triangle part at a steady 120 fps, fins drawn). Measured on the
M2 Max with three other agents running, so absolute numbers are noisy; A/B rows are interleaved runs, medians.
Merge this section into `docs/PERF-M5.md` when that file lands.

## The benchmark

`just bench-render [flags]` runs `viewer-bench` (`app/SupportFinsViewer/Sources/ViewerBench`), a headless
release-build benchmark. `StressScene` generates a lumpy sphere of 1-8M triangles (flat-shaded, every face
direction, overhang flags from the angle test) and a fin buffer of 3000 thin walls plus 6000 eight-sided tines
(about 190k triangles, with the pad) standing under it. `ViewerRenderer.benchmark` draws N frames offscreen
through the real colour pass (`encodeScene`), 2560x1600, three frames in flight as `MTKView` allows, and reports
the main-thread time to update and encode a frame (CPU), `gpuEndTime - gpuStartTime` (GPU), and the commit-to-commit
interval. Nothing waits for a display, so `fps` is the machine's ceiling, not what the display shows.

Scenarios: `orbit`, `panzoom`, `drag` (the pose turns every frame and a fresh 1 byte/face flags buffer arrives every
4 frames, as an analysis result does), `swap` (a new build replaces the fins and pad every 32 frames). The last line
is the latency of a face pick (the hover/click ID pass).

Debug builds also have View > Show Frame Time (the overlay of `FrameStats`; it was always on in debug before). The
live app can be timed with `SF_BENCH_SECONDS` (`app/SupportFins/Benchmark.swift`; with a sandboxed app,
`SF_BENCH_OUT` must be inside `~/Library/Containers/com.storskegg.SupportFins/Data`).

## Baseline (main at 8b2b9eb): the renderer is not the limit

1.5M triangles, 3000 fins, 6000 tines, 2560x1600, 600 frames (medians of repeated runs; GPU time per frame):

| scenario | CPU mean | GPU mean | GPU p95 | GPU max | frame interval p99 | fps ceiling |
|---|---|---|---|---|---|---|
| orbit | 0.02-0.03 ms | 1.1-1.5 ms | 1.6-2.4 ms | 1.8-3.1 ms | 1.0-2.2 ms | 950-1370 |
| panzoom | 0.01-0.03 ms | 1.1-1.5 ms | 1.5-2.1 ms | 2.0-2.7 ms | 1.0-1.9 ms | 930-1400 |
| drag (flags every 4 frames) | 0.06-0.12 ms | 1.2-1.6 ms | 1.5-2.1 ms | 1.8-2.7 ms | 1.1-2.1 ms | 900-1370 |

(The range is run-to-run variance on the shared machine; the A/B pairs below are the fair comparison.)

Scaling, orbit with fins: 990k triangles (no back-face culling, below `cullAbove`) 1.26 ms GPU; 2.0M 1.4 ms; 4.0M
2.4 ms; 7.7M (the size of the 10-object private Earth file) 4.0 ms GPU, 340 fps. Native resolution 3456x2234 adds
about 0.15 ms. Zoomed so the part fills the view adds nothing measurable. A 1.5M-triangle frame costs about 1/7 of
the 8.33 ms budget on the GPU and about 0.03 ms of main thread to encode: draw calls per frame are a handful (one per
part, one per fin segment, the plate lines), uniforms go in with `setVertexBytes`, and nothing is allocated per frame.
So instancing tines, indirect draws and triple-buffered uniforms have nothing to buy: the profile shows no encoder or
allocation cost to remove, and changing them would risk pixels for no gain. Not done, on that evidence.

The face pick (full ID pass over the part, one pixel read back) costs 1.0 ms at 1.5M triangles, 2.0 ms at 4M, 6.5 ms
mean (p95 20 ms) at 7.7M. It runs on mouse move only (never during an orbit or ring drag), coalesced to one in
flight.

## Live app (Release build, real MTKView at 120 Hz, 1.2M-triangle closed torus, no fins)

`SF_BENCH_SECONDS=10`: 119.9-120.0 fps, GPU mean 1.0-1.3 ms, p99 GPU 1.9-2.1 ms. The frame-interval p99 is 9-13 ms
and the worst 11-17 ms (about 1% of frames slip a display tick) in both builds below; the GPU is idle for 85% of each
frame, so these slips are main-thread or compositor time, not drawing.

### What the profile showed (Time Profiler, `xctrace record --attach`, 10 s of the orbit benchmark)

The main thread was 40% busy while orbiting. `ViewerRenderer.draw(in:)` was 87 ms of the 4177 ms (2%), `encodeScene`
44 ms. The rest was SwiftUI: the view cube redrew its two Canvases for every camera change, and each redraw
re-resolved its 6 + 3 text labels (`ViewCubeView.draw` 276 ms, `drawAxes` 444 ms, `GraphicsContext.resolve` 311 ms,
`CanvasDisplayList.updateValue` 777 ms inclusive), the Canvas rasterised in the CA commit on the main thread
(`RB::SharedSurfaceGroup::render_updates` 648 ms), and the whole `ViewCubeView` body, glass Home button included,
was re-evaluated per frame.

Fix (pixels unchanged: the view cube renders byte-identical in a before/after `ImageRenderer` PNG of the same camera):
labels are Canvas symbols (resolved once), both Canvases `rendersAsynchronously`, and the Home button is split out
of the view that reads `cameraRevision`.

| main thread, 10 s of orbit | before | after |
|---|---|---|
| `CanvasDisplayList.updateValue` | 777 ms | 135-326 ms |
| `ViewCubeView.drawAxes` | 444 ms | 23-46 ms |
| `GraphicsContext.resolve` | 311 ms | 0 |
| `RB::...render_updates` on main | 648 ms | 25 ms (off main) / 639 ms (symbols only) |
| `NSHostingView.layout` | 1603 ms | 821-1268 ms |
| `ViewCubeView.body` | 71 ms | 0 (not re-run) |
| all main-thread samples | 4177 | 3647-4041 |

The remaining SwiftUI cost (about 0.8 s per 10 s: the view-graph transaction every frame) is the cost of any
SwiftUI overlay that follows the camera. Live A/B (6 interleaved 10 s runs each): frame-interval p99 median 10.4 ms
base, 9.9 ms after; worst-frame median 15.8 ms vs 16.8 ms: a small gain in the tail that is inside the noise of the
shared machine. It removes 10% of a core of main-thread work, which is the headroom the 120 Hz callback needs; it
does not change fps.

## Live overhang shading during a ring drag

The viewer runs one analysis at a time off the main actor and the newest pose wins (`ViewerController.runFlags`),
so the colours trail the turn by one analysis, never block a frame, and need no throttle like the web's `SHADE_GAP`
(which exists because the web's `shade()` runs on the render thread). The part itself turns every frame on the
renderer's pose; the colours of the last finished analysis stay on it until the next lands.

Engine time per pose (`cargo run --release -p sf-ffi --example analyze_time -- <file>`, median of 7, serial engine):

| part | `analyze` | `face_flags` | the app's pair (before) | after |
|---|---|---|---|---|
| 1.2M-triangle torus | 19.1 ms | 20.5 ms | 39.6 ms | 20.5 ms |
| Earth Focus Many Vertical, one object (772,340 triangles) | 12.2 ms | 13.2 ms | 26.0 ms | 13.2 ms |

`CorePart.analysis` called `face_flags` and `analyze` back to back, and each runs the engine's `analyze`; a ring drag
reads only the flags (the pivot and box are the drag's own, `runFlags` ignores `offset`/`size` while dragging), so a
drag now skips the second call (`wantsSummary: false`): half the engine time per shade. The shading refresh rate goes
from about 25 Hz to about 50 Hz at 1.2M triangles (and 38 to 75 Hz at 772k). A multi-object file analyses its
objects in parallel; the ten-object Earth file is 10 x 13 ms of work spread over the cores. `analyze` itself is not
the bottleneck of the frame rate (it runs off the render thread and each shade is one 20 ms job), but it bounds how
fast the colours follow; a faster `analyze` (track B/C: rayon over faces) raises the refresh rate directly. The
release still runs the full analysis, which re-seats the part.

## Tried and not kept

- A persistent residency set updated by add/remove of changed allocations instead of re-making it on every new
  flags buffer or build (`rebuildResidency`). In the `swap` scenario the base showed a 10-14 ms interval spike in some
  runs; after the change fewer, but 5 interleaved runs of 1200 frames show medians identical (interval p99 1.96 vs
  2.01 ms) and spikes of 10+ ms in either build. Not shown to help, so not kept.
- Instancing for tines, indirect draws, triple-buffered uniforms, flags updated only on pose change: see the baseline;
  nothing to remove. (Flags already change only when a pose or threshold changes: `runFlags` is the only writer.)
- Picking on demand: hover picks already coalesce and run only on mouse move; at 1.5M triangles a pick is 1 ms of GPU.
  The inspector's face readout needs it.

## Reading the numbers

Headroom at the target: 1.5M triangles with 9000 fins and tines is 1.2 ms of GPU in an 8.33 ms frame; 7.7M triangles
is 4.0 ms. The frame rate at 120 fps holds on the renderer's side with margin; the remaining risk at 120 Hz is main-thread
time (SwiftUI, the build result handling), not the GPU.
