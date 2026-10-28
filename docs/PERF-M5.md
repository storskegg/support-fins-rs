# M5 timing: the fin build

The Rust `build_fins` per case, beside the M3 serial numbers ([PERF-M3.md](PERF-M3.md)). Regenerate the tables with `just bench-m5`
(Rust only; the JS and M3 serial columns are read from PERF-M3.md's tables). Timings on a shared machine are noisy: compare
A/B interleaved, and treat the final numbers as the quiet run's.

<!-- bench:begin -->

- **Engine threads:** 1 (machine reports 12 logical CPUs). **Rust:** rustc 1.99.1 (58cf8f9c3 2026-10-13), `cargo build --release`. **Runs:** one discarded warm-up, then the median of 5 (3 for the private objects).
- **out verts** is the build's output vertex count, recorded so a change that alters output shows here (the parity corpus is the real gate).

### Summary

- Stress models at golden poses: 48 builds, total M3 serial 613 ms vs now 608 ms (1.01x), median build now 7.73 ms
- Worst orientations: 29 builds, total M3 serial 714 ms vs now 705 ms (1.01x), median build now 14.9 ms
- Private objects: 10 builds, total M3 serial 9367 ms vs now 9415 ms (0.99x), median build now 944 ms

### Slowest 12 stress builds (the proxies)

| model | pose | triangles | out verts | JS ms (M3) | Rust ms (M3 serial) | Rust ms (now) | ratio vs M3 serial |
|---|---|---:|---:|---:|---:|---:|---:|
| needle | X60Z30 | 384 | 8712 | 119 | 68.0 | 65.5 | 1.04x |
| needle | X60 | 384 | 8712 | 119 | 65.4 | 63.3 | 1.03x |
| cone | X40 | 96 | 11352 | 124 | 61.2 | 61.2 | 1.00x |
| tube | X60Z30 | 384 | 12396 | 88.2 | 52.7 | 53.2 | 0.99x |
| needle | X30 | 384 | 12816 | 97.9 | 51.6 | 51.1 | 1.01x |
| torus | X45 | 2048 | 8460 | 98.8 | 51.1 | 49.7 | 1.03x |
| needle | X40 | 384 | 11352 | 94.4 | 48.7 | 48.5 | 1.00x |
| torus | Y45 | 2048 | 8028 | 89.8 | 48.2 | 47.4 | 1.02x |
| torus | X45Y30 | 2048 | 4236 | 75.7 | 40.8 | 40.4 | 1.01x |
| ushape | X40 | 28 | 9852 | 80.1 | 41.3 | 39.7 | 1.04x |
| coin | X40 | 256 | 22980 | 72.0 | 37.1 | 37.6 | 0.99x |
| coin | X30 | 256 | 25848 | 69.0 | 37.3 | 37.3 | 1.00x |

### Stress models at golden poses

| model | pose | triangles | out verts | JS ms (M3) | Rust ms (M3 serial) | Rust ms (now) | ratio vs M3 serial |
|---|---|---:|---:|---:|---:|---:|---:|
| cube | X20 | 12 | 9792 | 31.3 | 13.4 | 13.5 | 0.99x |
| cube | X30 | 12 | 9312 | 26.0 | 12.2 | 12.2 | 1.00x |
| cube | X40 | 12 | 8448 | 30.8 | 11.6 | 11.3 | 1.03x |
| lbracket | X35 | 20 | 7788 | 30.0 | 13.5 | 13.7 | 0.99x |
| sphere | up | 1280 | 5112 | 20.8 | 10.4 | 10.5 | 0.99x |
| staircase | X40 | 28 | 6384 | 45.1 | 21.6 | 21.9 | 0.99x |
| arch | up | 20 | 0 | 0.05 | 0.01 | 0.01 | 1.80x |
| arch | X40 | 20 | 8976 | 29.4 | 15.0 | 15.3 | 0.98x |
| bar | up | 12 | 0 | 0.04 | 0.00 | 0.00 | 0.00x |
| bar | X40 | 12 | 2700 | 9.49 | 4.44 | 4.68 | 0.95x |
| coin | up | 256 | 0 | 0.37 | 0.03 | 0.03 | 1.11x |
| coin | X40 | 256 | 22980 | 72.0 | 37.1 | 37.6 | 0.99x |
| cone | up | 96 | 0 | 0.07 | 0.01 | 0.02 | 0.57x |
| cone | X40 | 96 | 11352 | 124 | 61.2 | 61.2 | 1.00x |
| cube | up | 12 | 0 | 0.02 | 0.01 | 0.00 | 2.33x |
| cylinder | up | 192 | 0 | 0.18 | 0.02 | 0.02 | 0.93x |
| cylinder | X40 | 192 | 5184 | 14.9 | 7.81 | 7.73 | 1.01x |
| hexprism | up | 20 | 0 | 0.02 | 0.01 | 0.01 | 1.82x |
| hexprism | X40 | 20 | 4236 | 14.2 | 7.03 | 7.22 | 0.97x |
| lbracket | up | 20 | 0 | 0.02 | 0.01 | 0.01 | 1.90x |
| lbracket | X40 | 20 | 7428 | 65.8 | 31.6 | 32.1 | 0.98x |
| lowledge | up | 20 | 12180 | 82.7 | 36.7 | 37.2 | 0.99x |
| lowledge | X40 | 20 | 14784 | 17.0 | 7.63 | 7.71 | 0.99x |
| needle | up | 384 | 0 | 0.53 | 0.05 | 0.05 | 1.07x |
| needle | X40 | 384 | 11352 | 94.4 | 48.7 | 48.5 | 1.00x |
| plate | up | 12 | 0 | 0.02 | 0.01 | 0.01 | 1.88x |
| plate | X40 | 12 | 24960 | 50.6 | 26.8 | 26.1 | 1.03x |
| plus | up | 44 | 0 | 0.03 | 0.01 | 0.01 | 1.23x |
| plus | X40 | 44 | 6696 | 13.3 | 7.20 | 7.15 | 1.01x |
| portal | up | 32 | 0 | 0.03 | 0.01 | 0.01 | 1.57x |
| portal | X40 | 32 | 4116 | 31.8 | 16.4 | 16.3 | 1.01x |
| pyramid | up | 8 | 0 | 0.02 | 0.00 | 0.00 | 0.00x |
| pyramid | X40 | 8 | 7932 | 6.70 | 3.66 | 3.69 | 0.99x |
| ramp | up | 16 | 0 | 0.02 | 0.01 | 0.00 | 2.09x |
| ramp | X40 | 16 | 10656 | 46.5 | 25.8 | 25.4 | 1.02x |
| sphere | X40 | 1280 | 5196 | 20.2 | 11.4 | 11.3 | 1.01x |
| staircase | up | 28 | 0 | 0.02 | 0.01 | 0.01 | 1.60x |
| torus | up | 2048 | 0 | 44.3 | 24.5 | 24.4 | 1.00x |
| torus | X40 | 2048 | 8592 | 61.7 | 33.5 | 32.6 | 1.03x |
| tshape | up | 28 | 0 | 0.02 | 0.01 | 0.01 | 1.60x |
| tshape | X40 | 28 | 8928 | 33.6 | 18.0 | 17.4 | 1.04x |
| tube | up | 384 | 0 | 0.14 | 0.04 | 0.04 | 1.03x |
| tube | X40 | 384 | 3072 | 23.1 | 12.6 | 12.0 | 1.05x |
| ushape | up | 28 | 0 | 0.03 | 0.01 | 0.01 | 1.63x |
| ushape | X40 | 28 | 9852 | 80.1 | 41.3 | 39.7 | 1.04x |
| wedge | up | 8 | 0 | 0.02 | 0.00 | 0.00 | 0.00x |
| wedge | X40 | 8 | 6108 | 30.8 | 16.7 | 16.0 | 1.04x |
| torus | X30 | 2048 | 10824 | 59.8 | 34.9 | 33.9 | 1.03x |

### Worst orientations

| model | pose | triangles | out verts | JS ms (M3) | Rust ms (M3 serial) | Rust ms (now) | ratio vs M3 serial |
|---|---|---:|---:|---:|---:|---:|---:|
| torus | X45 | 2048 | 8460 | 98.8 | 51.1 | 49.7 | 1.03x |
| torus | X60 | 2048 | 3900 | 60.3 | 28.8 | 28.2 | 1.02x |
| torus | Y45 | 2048 | 8028 | 89.8 | 48.2 | 47.4 | 1.02x |
| torus | X45Y30 | 2048 | 4236 | 75.7 | 40.8 | 40.4 | 1.01x |
| torus | X60Z30 | 2048 | 3840 | 59.3 | 28.9 | 28.9 | 1.00x |
| sphere | X30 | 1280 | 5280 | 19.1 | 11.4 | 11.3 | 1.01x |
| sphere | X45 | 1280 | 5436 | 24.2 | 14.3 | 14.5 | 0.99x |
| sphere | X60 | 1280 | 1824 | 19.9 | 12.0 | 12.1 | 0.99x |
| sphere | Y45 | 1280 | 5436 | 23.8 | 14.8 | 15.0 | 0.98x |
| sphere | X45Y30 | 1280 | 5316 | 19.2 | 11.9 | 12.2 | 0.97x |
| sphere | X60Z30 | 1280 | 5448 | 20.4 | 12.5 | 12.4 | 1.01x |
| needle | X30 | 384 | 12816 | 97.9 | 51.6 | 51.1 | 1.01x |
| needle | X45 | 384 | 720 | 6.03 | 3.12 | 3.28 | 0.95x |
| needle | X60 | 384 | 8712 | 119 | 65.4 | 63.3 | 1.03x |
| needle | Y45 | 384 | 720 | 5.83 | 3.23 | 3.19 | 1.01x |
| needle | X45Y30 | 384 | 2604 | 47.1 | 23.9 | 23.6 | 1.01x |
| needle | X60Z30 | 384 | 8712 | 119 | 68.0 | 65.5 | 1.04x |
| tube | X30 | 384 | 4800 | 26.6 | 15.3 | 14.9 | 1.03x |
| tube | X45 | 384 | 2832 | 10.8 | 6.29 | 6.05 | 1.04x |
| tube | X60 | 384 | 12396 | 66.4 | 36.6 | 36.3 | 1.01x |
| tube | Y45 | 384 | 2832 | 10.6 | 6.26 | 6.08 | 1.03x |
| tube | X45Y30 | 384 | 10824 | 62.6 | 35.6 | 35.2 | 1.01x |
| tube | X60Z30 | 384 | 12396 | 88.2 | 52.7 | 53.2 | 0.99x |
| coin | X30 | 256 | 25848 | 69.0 | 37.3 | 37.3 | 1.00x |
| coin | X45 | 256 | 1092 | 10.4 | 6.82 | 6.84 | 1.00x |
| coin | X60 | 256 | 984 | 19.2 | 7.23 | 7.17 | 1.01x |
| coin | Y45 | 256 | 696 | 10.1 | 7.04 | 6.93 | 1.02x |
| coin | X45Y30 | 256 | 1044 | 10.7 | 6.81 | 6.70 | 1.02x |
| coin | X60Z30 | 256 | 984 | 9.70 | 6.22 | 6.26 | 0.99x |

### Private coin file (Earth Focus Many Vertical.3mf)

Each object at the pose it has in the file; only the name, counts and timings are recorded.

| object | triangles | out verts | M3 serial ms | now ms | ratio |
|---|---:|---:|---:|---:|---:|
| object 1 | 772340 | 576 | 959 | 956 | 1.00x |
| object 2 | 772340 | 576 | 934 | 931 | 1.00x |
| object 3 | 772340 | 576 | 940 | 951 | 0.99x |
| object 4 | 772340 | 576 | 923 | 931 | 0.99x |
| object 5 | 772340 | 576 | 933 | 944 | 0.99x |
| object 6 | 772340 | 576 | 942 | 946 | 1.00x |
| object 7 | 772340 | 576 | 933 | 951 | 0.98x |
| object 8 | 772340 | 576 | 927 | 934 | 0.99x |
| object 9 | 772340 | 576 | 936 | 942 | 0.99x |
| object 10 | 772340 | 576 | 940 | 931 | 1.01x |

<!-- bench:end -->

## Where the build time goes (profile, serial, before M5)

Method: macOS `sample` (1 ms call trees of 100-400 builds of one case) to find the functions, then wall-clock timers around the pipeline stages (uncommitted instrumentation; the rows are inclusive and nested, so they do not add up) on a quiet-ish machine, one build per case. The upstream spec's "60% unattributed" is attributed below.

| stage (inclusive) | cone X40 | torus X45 | coin X40 | needle X60Z30 | ushape X40 | lowledge up | private object 1 |
|---|---:|---:|---:|---:|---:|---:|---:|
| build_fins total ms | 62.7 | 50.9 | 37.9 | 69.3 | 41.3 | 38.4 | 973 |
| automatic_targets (classify, tiny-hole floors) | 0% | 1% | 0% | 0% | 0% | 0% | 50% |
| judge: downhill build | 59% | 59% | 57% | 19% | 53% | 93% | 44% |
| judge: plain build (same base again) | 33% | 32% | - | - | 42% | - | - |
| build_props | 91% | 90% | 57% | 18% | 46% | 93% | 25% |
| - normal pass | 31% | 24% | 54% | 17% | 16% | 41% | 12% |
| - raster race | 33% | 36% | 0% | 0% | 17% | 0% | 5% |
| - rim race | 1% | 1% | 2% | 1% | 1% | 52% | 1% |
| - downhill race | 25% | 25% | 0% | 0% | 11% | 0% | 4% |
| per-wall: solid_clearance (finished-wall weld check) | 41% | 28% | 23% | 7% | 15% | squat bed | 1% |
| per-wall: station_certified | 16% | 10% | 12% | 2% | 8% | squat bed | 0% |
| per-wall: station_is_clear | 12% | 16% | 11% | 2% | 8% | 6% | 1% |
| per-wall: part_attached (settle_top, floor_line) | 4% | 10% | 3% | 4% | 3% | 3% | 12% |
| per-wall: emit_tines | 3% | 3% | 2% | 1% | 3% | - | 6% |
| bed pad (brim lattice) | 0% | 0% | 0% | 1% | 50% | - | 0% |
| bare_area (fill.rs) | 8% | 6% | 40% | 22% | 4% | 4% | 4% |
| last_resort_walls | 0% | 0% | 0% | 0% | 0% | 0% | 12% |
| floating_pieces | 0% | 0% | 0% | 0% | 0% | 0% | 3% |

(needle X60Z30's bare_area was 15 ms of 30 ms on a quiet run: 50%. Percentages are of that case's `build_fins` time in the instrumented run.)

What the profile says, in the order it was acted on:

1. **`bare_area` re-walks every red triangle twice** and spends its time in `Reach::near` (SipHash grid lookups, 300k calls on needle X60Z30) and the std hasher: 57% of needle X60Z30, 25% of coin X40.
2. **Auto's judge builds everything twice** when a region swapped to a downhill wall, and the second build recomputes the same base (normal pass, raster, rim) that the downhill flag does not touch: a third of cone/torus/ushape.
3. **The finished-wall weld check (`inside::solid_clearance`) is the largest primitive** (41% of cone X40), then the per-station probes (`station_certified` -> `nearest_part`, `station_is_clear` -> `inside_part`): about 70% of a cone build, inside one or two walls (the stress parts have 1-3 patches per pass, so the parallelism has to be inside a wall, not across walls).
4. **The bed pad's brim lattice** is half of ushape X40: ~42,000 `top(x, y)` calls of 17 surface lookups each.
5. **On the 772k-triangle private objects**, half the build is `automatic_targets`: a floor probe under each of the excluded tiny-hole faces (~430 ms), five 772k-triangle surface grids (36 ms each, one per pass because `build_pass` seated its own copy of the part), and a lazily built 64x64 inside grid.

Not changed (and why): the per-wall corridor early-out (strategy 2) is not exact, since `inside_part` is a ray-parity test whose answer in an empty box depends on open meshes and on rays that graze an edge; the weld-regression station and z-probe steps are untouched (strategy 6 is off the table); merging `station_is_clear` and `station_certified` (strategy 3) is not possible, since the second runs on the line after `settle_top` has moved it.

## Live drag

Builds are cancellable (`sf_engine::cancel`; `build_fins_cancellable`; `Builder` raises the flag of the build in flight when a newer one arrives or `cancel()` is called), so a live rebuild during a ring drag can abandon a stale pose part-way.

### Cancel latency

`just bench-cancel` (`bench_cancel`): each case is built on a warm topology (a drag rebuilds on one), timed three times uncancelled, then built 16 times (8 on the private objects, objects 1, 4, 7 and 10) with a helper thread raising the flag at evenly spread fractions of the build time. The latency is the time from the flag store to the return of `build_fins_cancellable` (`Err(Cancelled)`). A trial whose build finished before its flag counts as completed and is left out. Idle-ish shared machine (load 4-7), one run; the orchestrator records the quiet numbers.

| case | build ms | stopped | median ms | p95 ms | max ms |
|---|---:|---:|---:|---:|---:|
| needle X60Z30 | 18.3 | 12/16 | 0.41 | 1.04 | 1.17 |
| needle X60 | 14.8 | 11/16 | 0.21 | 1.10 | 1.10 |
| cone X40 | 18.8 | 15/16 | 0.19 | 1.56 | 2.45 |
| tube X60Z30 | 33.6 | 15/16 | 0.45 | 2.39 | 3.74 |
| torus X45 | 23.3 | 16/16 | 0.22 | 0.86 | 1.65 |
| ushape X40 | 19.0 | 16/16 | 0.13 | 0.25 | 0.85 |
| coin X40 | 12.7 | 13/16 | 0.21 | 0.49 | 0.56 |
| lowledge up | 23.8 | 16/16 | 1.46 | 4.02 | 5.43 |
| Earth Focus Many Vertical object 1 | 267.8 | 7/8 | 9.62 | 19.09 | 19.09 |
| Earth Focus Many Vertical object 4 | 267.6 | 8/8 | 0.15 | 11.22 | 11.22 |
| Earth Focus Many Vertical object 7 | 255.9 | 8/8 | 0.37 | 13.11 | 13.11 |
| Earth Focus Many Vertical object 10 | 256.2 | 8/8 | 0.25 | 10.32 | 10.32 |

All 87 cases together (1092 stopped builds): median 0.13 ms, p95 2.2 ms, p99 5.7 ms, max 19 ms (the private objects; the stress models stay under 6 ms).

What bounds the latency, found with a poll-gap profile (the longest time between two polls on the calling thread): the pad's surface probes, the part-attached and clearance checks of one wall, the wedge column and the raster race's per-region loop (all poll inside them now), and on the 772k-triangle objects the stages that are one parallel pass over every face (the first inside-grid build in `classify`, the seated-part copy, `find_wall_patches`' seed sort) which only poll between them (10-20 ms).

Two design points the measurements forced:
- A poll inside a Rayon closure must not unwind: every job that sees the raised flag would unwind separately, and a 13 ms build then took 50 ms to stop. Inside a parallel loop a raised flag makes the remaining items return a placeholder, and one poll after the loop unwinds once (cancel latency of that build: 0.09 ms median, 0.9 ms max).
- The polls cost 0.9% of the 87-build total (median of five interleaved runs, 4249 ms before vs 4285 ms after). Output hashes of all 87 builds are identical to main's.
