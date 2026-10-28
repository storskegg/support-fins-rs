# M5 live drag: rebuilding the fins while a ring is dragged

Track E. Merge into `docs/PERF-M5.md` ("Live drag"; that section already holds the cancel latency from the
engine PR) when both have landed. Measured on the M2 Max with other agents running: noisy, one run per cell.

## What the app does

The web rebuilds the fins when a ring drag ends (`web/ui/pose.js`: "Fins are rebuilt when the drag ENDS", ~100 ms on a
43k-face part). With builds this fast the app rebuilds during the drag (View > Rebuild While Dragging, on by default,
remembered under `sf.rebuildWhileDragging`; off is the web's behaviour: nothing is built until the release, then at once).

- A new pose while a ring is down starts a build at once (no debounce; the 180 ms debounce stays for settings and for a
  turn made by a button).
- At most one build is in flight. A new pose supersedes it (the engine stops the stale build part-way,
  [cancel latency](PERF-M5.md)) **only while the build is younger than 8 ms** (`BuildController.liveLandAfter`);
  an older build is let land, and the newest pose is built next.
- The fins on screen are drawn pale (the existing stale styling, opacity 0.25) only while they lag the pose: a build is
  out and the fins shown were made for another pose. The release does the final build at the released pose at once
  (the live build already in flight for that exact request is kept). One undo step per drag, as before.

### Why 8 ms and not "a new pose cancels the build in flight"

`cargo run --release -p sf-ffi --example bench_drag`: a drag simulated as 60 Hz pose events (X from 0 to the pose
named, 1.5 s) through a `Builder` under the policy in question, the real build path of the app (`build_part`, the posed
part re-welded and analysed per pose). "lag" is the angle between the pose of the ring and the pose of the fins on
screen at each event.

| model | policy | one build ms | landed/started | fins lag median deg | lag max deg | last pose to final fins ms |
|---|---|---:|---:|---:|---:|---:|
| needle X60 | always supersede | 21.3 | 16/90 | 14.8 | 44.5 | 21.6 |
| needle X60 | 8 ms | 20.8 | 67/86 | 1.3 | 3.4 | 21.9 |
| needle X60 | never supersede | 20.8 | 73/73 | 1.3 | 4.0 | 41.1 |
| cone X40 | always supersede | 17.1 | 3/90 | 19.8 | 39.6 | 17.5 |
| cone X40 | 8 ms | 17.0 | 65/80 | 0.9 | 2.2 | 23.2 |
| cone X40 | never supersede | 17.1 | 72/72 | 0.9 | 2.2 | 29.7 |
| torus X45 | always supersede | 22.7 | 1/90 | 22.8 | 45.0 | 22.8 |
| torus X45 | 8 ms | 22.5 | 46/83 | 1.5 | 2.5 | 22.7 |
| torus X45 | never supersede | 22.6 | 54/54 | 1.5 | 2.5 | 38.6 |
| coin X40 | always supersede | 13.8 | 56/90 | 0.4 | 15.7 | 13.4 |
| coin X40 | 8 ms | 13.4 | 74/77 | 0.4 | 4.5 | 13.6 |
| lowledge X30 | always supersede | 6.3 | 60/90 | 0.3 | 10.1 | 6.3 |
| lowledge X30 | 8 ms | 6.1 | 77/88 | 0.3 | 1.3 | 6.3 |
| private object 1 (772k tris), X30 | always supersede | 393.6 | 1/90 | 15.2 | 30.0 | 417.8 |
| private object 1 (772k tris), X30 | 8 ms | 388.1 | 5/7 | 10.4 | 16.5 | 472.5 |
| private object 1 (772k tris), X30 | never supersede | 385.3 | 6/6 | 9.8 | 15.5 | 771.8 |

Always superseding starves whenever a build is slower than the events (a 22 ms torus against 17 ms events lands one
build in 90, and the fins sit 23 degrees behind the ring until it stops). Never superseding lands every build but the
final fins can be two builds late. 8 ms keeps the fins within 1-5 degrees of the ring on every stress proxy and the
release no later than a single build. At 120 Hz events (HZ=120): the same ordering (torus 44/87 landed, median lag 1.5
degrees; always-supersede 1/180). 25 ms and 100 ms behave as "always" for builds shorter than that.

What a live drag costs per pose, on the app's path: the proxies build in 6-23 ms (`one build ms`, which includes the
per-pose weld and analysis); the 772k-triangle private object takes 390 ms, 120 ms of it the per-pose weld of the posed
copy (`compute_fins_detail`), so on a part that size the fins trail the ring by about 10 degrees and the release waits
one build. Landing a build on the main actor (decoding the summary and the records) is 0.2 ms on needle X60 (8712 fin
vertices, 10 KB of records).

## Frame time while builds land (`just bench-render --scenario liveDrag`)

New `liveDrag` scenario of `viewer-bench`: the pose turns every frame and every 4 frames a new build replaces the fins
and pad (alternating two layers of 3000 walls + 6000 tines) with a fresh flags buffer: about 30 swaps a second at 120 fps,
1.5M-triangle part, 2560x1600, 600 frames, three frames in flight (medians of three runs):

| scenario | CPU mean | GPU mean | GPU p99 | frame interval p99 | interval max |
|---|---:|---:|---:|---:|---:|
| drag (flags every 4 frames, fins fixed) | 0.05 ms | 1.04 ms | 1.34 ms | 0.89 ms | 1.09 ms |
| liveDrag (a new build every 4 frames) | 0.06 ms | 1.18 ms | 1.64 ms | 1.25 ms | 1.34 ms |

The fins swap costs 0.1 ms of GPU (two alternating fin sets are different geometry) and no frame stalls. Also run once
with `MTL_DEBUG_LAYER=1` (Metal API validation): no errors with 100 swaps a second.

### The swap is already safe, so it is not double-buffered

A landing build is a new immutable buffer pair (`BuildResult.fins()` / `pad()`, the Rust allocation wrapped without a
copy); nothing is written in place, `setSupports` replaces the reference, and the command buffers already committed keep
the buffers they encoded alive (and a `GpuBuffer` frees the Rust memory only when its `MTLBuffer` is released, after
those command buffers complete: `theMetalBufferKeepsTheRustMemoryAlive`). The residency set is rebuilt per swap
(`rebuildResidency`); an earlier attempt to update it incrementally was not shown to help (docs/PERF-M5-rendering.md,
"Tried and not kept"). Another buffer on top would add memory (two sets of fins) and nothing the measurement shows.

## Tried and not kept

- "A new pose cancels the build in flight", always: see above.
- Double-buffering the fins (above): nothing to fix.
