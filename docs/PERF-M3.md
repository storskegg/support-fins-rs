# M3 timing: JS engine vs single-threaded Rust engine

The serial baseline M5 improves on. Regenerate with `just bench-m3` (writes this file).

- **Machine:** Apple M2 Max, macOS 26.6.2.
- **JS:** Deno 2.9.8, `buildFins` from the pinned upstream (299ef49). **Rust:** rustc 1.99.1 (58cf8f9c3 2026-10-13), `cargo build --release`, `build_fins`, one thread.
- **Builds:** the site's own request (`tests/_scene.js` `siteOpts()`: Auto, PLA, tines, bed pad, the form untouched), threshold 45.
- **Method:** each case is timed 5 times after one discarded warm-up; the table gives the median. The phases are separate:
  **load** = read the file and parse it into a position soup, **weld** = `buildTopology` + `analyze`, and the **JS ms / Rust ms** columns are `buildFins` alone (the ratio is JS over Rust, so above 1 means Rust is faster).
  Every run starts from a fresh topology, so lazily built grids are charged to the build.
- **Same work:** the Rust build produced the same support vertex count as JS in 77 of 77 compared cases. Both engines are tested for exact equality in the parity corpus, so these timings compare equal work.

## Summary

- Stress models at their golden poses: 48 builds, total JS 1212 ms vs Rust 613 ms (2.0x overall, median per-build ratio 2.2x)
- Worst orientations of the torus, sphere, needle, tube, coin models: 29 builds, total JS 1299 ms vs Rust 714 ms (1.8x overall, median per-build ratio 1.7x)
- Private coin file: 10 Rust builds timed; the JS engine cannot load the file (see below).

## Stress models at their golden poses

Every stress model at the poses of its non-draw, non-sway `tests/golden` scenes ("up" = as modelled, `X40` = tilted 40 degrees), at default settings.

| model | pose | triangles | JS ms | Rust ms | ratio | JS load / weld ms | Rust load / weld ms |
|---|---|---:|---:|---:|---:|---:|---:|
| cube | X20 | 12 | 31.3 | 13.4 | 2.3x | 0.18 / 0.06 | 0.08 / 0.01 |
| cube | X30 | 12 | 26.0 | 12.2 | 2.1x | 0.18 / 0.04 | 0.07 / 0.01 |
| cube | X40 | 12 | 30.8 | 11.6 | 2.7x | 0.19 / 0.04 | 0.05 / 0.01 |
| lbracket | X35 | 20 | 30.0 | 13.5 | 2.2x | 0.19 / 0.05 | 0.06 / 0.01 |
| sphere | up | 1280 | 20.8 | 10.4 | 2.0x | 0.31 / 0.27 | 0.07 / 0.34 |
| staircase | X40 | 28 | 45.1 | 21.6 | 2.1x | 0.18 / 0.02 | 0.08 / 0.01 |
| arch | up | 20 | 0.05 | 0.01 | 8.2x | 0.04 / 0.01 | 0.03 / 0.00 |
| arch | X40 | 20 | 29.4 | 15.0 | 2.0x | 0.16 / 0.02 | 0.06 / 0.01 |
| bar | up | 12 | 0.04 | 0.00 | 8.4x | 0.05 / 0.01 | 0.03 / 0.00 |
| bar | X40 | 12 | 9.49 | 4.44 | 2.1x | 0.18 / 0.02 | 0.04 / 0.00 |
| coin | up | 256 | 0.37 | 0.03 | 13.9x | 0.07 / 0.21 | 0.03 / 0.04 |
| coin | X40 | 256 | 72.0 | 37.1 | 1.9x | 0.22 / 0.07 | 0.09 / 0.06 |
| cone | up | 96 | 0.07 | 0.01 | 4.6x | 0.05 / 0.02 | 0.03 / 0.02 |
| cone | X40 | 96 | 124 | 61.2 | 2.0x | 0.20 / 0.04 | 0.10 / 0.03 |
| cube | up | 12 | 0.02 | 0.01 | 4.2x | 0.04 / 0.01 | 0.03 / 0.00 |
| cylinder | up | 192 | 0.18 | 0.02 | 8.3x | 0.06 / 0.04 | 0.03 / 0.03 |
| cylinder | X40 | 192 | 14.9 | 7.81 | 1.9x | 0.15 / 0.05 | 0.06 / 0.04 |
| hexprism | up | 20 | 0.02 | 0.01 | 3.6x | 0.04 / 0.01 | 0.03 / 0.00 |
| hexprism | X40 | 20 | 14.2 | 7.03 | 2.0x | 0.17 / 0.02 | 0.05 / 0.01 |
| lbracket | up | 20 | 0.02 | 0.01 | 3.8x | 0.04 / 0.01 | 0.03 / 0.00 |
| lbracket | X40 | 20 | 65.8 | 31.6 | 2.1x | 0.19 / 0.02 | 0.08 / 0.01 |
| lowledge | up | 20 | 82.7 | 36.7 | 2.3x | 0.18 / 0.02 | 0.08 / 0.01 |
| lowledge | X40 | 20 | 17.0 | 7.63 | 2.2x | 0.16 / 0.02 | 0.05 / 0.01 |
| needle | up | 384 | 0.53 | 0.05 | 11.0x | 0.10 / 0.08 | 0.03 / 0.07 |
| needle | X40 | 384 | 94.4 | 48.7 | 1.9x | 0.20 / 0.09 | 0.11 / 0.08 |
| plate | up | 12 | 0.02 | 0.01 | 3.5x | 0.03 / 0.01 | 0.03 / 0.00 |
| plate | X40 | 12 | 50.6 | 26.8 | 1.9x | 0.18 / 0.02 | 0.09 / 0.01 |
| plus | up | 44 | 0.03 | 0.01 | 4.0x | 0.04 / 0.01 | 0.03 / 0.01 |
| plus | X40 | 44 | 13.3 | 7.20 | 1.8x | 0.14 / 0.02 | 0.05 / 0.01 |
| portal | up | 32 | 0.03 | 0.01 | 3.6x | 0.03 / 0.01 | 0.03 / 0.01 |
| portal | X40 | 32 | 31.8 | 16.4 | 1.9x | 0.16 / 0.02 | 0.06 / 0.01 |
| pyramid | up | 8 | 0.02 | 0.00 | 4.8x | 0.04 / 0.01 | 0.02 / 0.00 |
| pyramid | X40 | 8 | 6.70 | 3.66 | 1.8x | 0.12 / 0.01 | 0.04 / 0.00 |
| ramp | up | 16 | 0.02 | 0.01 | 4.1x | 0.03 / 0.01 | 0.02 / 0.00 |
| ramp | X40 | 16 | 46.5 | 25.8 | 1.8x | 0.18 / 0.02 | 0.08 / 0.01 |
| sphere | X40 | 1280 | 20.2 | 11.4 | 1.8x | 0.19 / 0.22 | 0.09 / 0.34 |
| staircase | up | 28 | 0.02 | 0.01 | 2.9x | 0.03 / 0.01 | 0.03 / 0.01 |
| torus | up | 2048 | 44.3 | 24.5 | 1.8x | 0.20 / 0.24 | 0.10 / 0.40 |
| torus | X40 | 2048 | 61.7 | 33.5 | 1.8x | 0.23 / 0.26 | 0.12 / 0.42 |
| tshape | up | 28 | 0.02 | 0.01 | 2.9x | 0.03 / 0.01 | 0.03 / 0.01 |
| tshape | X40 | 28 | 33.6 | 18.0 | 1.9x | 0.18 / 0.02 | 0.10 / 0.01 |
| tube | up | 384 | 0.14 | 0.04 | 3.5x | 0.05 / 0.05 | 0.03 / 0.06 |
| tube | X40 | 384 | 23.1 | 12.6 | 1.8x | 0.20 / 0.07 | 0.12 / 0.08 |
| ushape | up | 28 | 0.03 | 0.01 | 4.0x | 0.05 / 0.01 | 0.03 / 0.01 |
| ushape | X40 | 28 | 80.1 | 41.3 | 1.9x | 0.19 / 0.02 | 0.11 / 0.01 |
| wedge | up | 8 | 0.02 | 0.00 | 3.1x | 0.03 / 0.01 | 0.03 / 0.00 |
| wedge | X40 | 8 | 30.8 | 16.7 | 1.8x | 0.17 / 0.02 | 0.07 / 0.01 |
| torus | X30 | 2048 | 59.8 | 34.9 | 1.7x | 0.24 / 0.26 | 0.11 / 0.42 |

## Worst orientations

The five stress models with the most triangles (torus, sphere, needle, tube, coin) in each pose of `prototype/stress/run.js` (flat, X30, X45, X60, Y45, X45Y30, X60Z30), other than the ones above.

| model | pose | triangles | JS ms | Rust ms | ratio | JS load / weld ms | Rust load / weld ms |
|---|---|---:|---:|---:|---:|---:|---:|
| torus | X45 | 2048 | 98.8 | 51.1 | 1.9x | 0.23 / 0.25 | 0.12 / 0.41 |
| torus | X60 | 2048 | 60.3 | 28.8 | 2.1x | 0.22 / 0.25 | 0.12 / 0.41 |
| torus | Y45 | 2048 | 89.8 | 48.2 | 1.9x | 0.23 / 0.26 | 0.14 / 0.42 |
| torus | X45Y30 | 2048 | 75.7 | 40.8 | 1.9x | 0.24 / 0.26 | 0.14 / 0.42 |
| torus | X60Z30 | 2048 | 59.3 | 28.9 | 2.1x | 0.26 / 0.27 | 0.11 / 0.40 |
| sphere | X30 | 1280 | 19.1 | 11.4 | 1.7x | 0.19 / 0.19 | 0.07 / 0.35 |
| sphere | X45 | 1280 | 24.2 | 14.3 | 1.7x | 0.20 / 0.19 | 0.08 / 0.34 |
| sphere | X60 | 1280 | 19.9 | 12.0 | 1.7x | 0.18 / 0.19 | 0.07 / 0.34 |
| sphere | Y45 | 1280 | 23.8 | 14.8 | 1.6x | 0.20 / 0.19 | 0.08 / 0.35 |
| sphere | X45Y30 | 1280 | 19.2 | 11.9 | 1.6x | 0.17 / 0.20 | 0.07 / 0.36 |
| sphere | X60Z30 | 1280 | 20.4 | 12.5 | 1.6x | 0.18 / 0.19 | 0.08 / 0.34 |
| needle | X30 | 384 | 97.9 | 51.6 | 1.9x | 0.17 / 0.07 | 0.12 / 0.09 |
| needle | X45 | 384 | 6.03 | 3.12 | 1.9x | 0.12 / 0.06 | 0.05 / 0.07 |
| needle | X60 | 384 | 119 | 65.4 | 1.8x | 0.19 / 0.07 | 0.12 / 0.08 |
| needle | Y45 | 384 | 5.83 | 3.23 | 1.8x | 0.13 / 0.06 | 0.05 / 0.07 |
| needle | X45Y30 | 384 | 47.1 | 23.9 | 2.0x | 0.18 / 0.07 | 0.12 / 0.08 |
| needle | X60Z30 | 384 | 119 | 68.0 | 1.7x | 0.20 / 0.07 | 0.10 / 0.08 |
| tube | X30 | 384 | 26.6 | 15.3 | 1.7x | 0.13 / 0.06 | 0.10 / 0.08 |
| tube | X45 | 384 | 10.8 | 6.29 | 1.7x | 0.13 / 0.06 | 0.08 / 0.08 |
| tube | X60 | 384 | 66.4 | 36.6 | 1.8x | 0.18 / 0.06 | 0.12 / 0.09 |
| tube | Y45 | 384 | 10.6 | 6.26 | 1.7x | 0.14 / 0.06 | 0.07 / 0.07 |
| tube | X45Y30 | 384 | 62.6 | 35.6 | 1.8x | 0.20 / 0.06 | 0.10 / 0.08 |
| tube | X60Z30 | 384 | 88.2 | 52.7 | 1.7x | 0.20 / 0.06 | 0.11 / 0.09 |
| coin | X30 | 256 | 69.0 | 37.3 | 1.9x | 0.19 / 0.05 | 0.11 / 0.06 |
| coin | X45 | 256 | 10.4 | 6.82 | 1.5x | 0.12 / 0.04 | 0.07 / 0.05 |
| coin | X60 | 256 | 19.2 | 7.23 | 2.7x | 0.12 / 0.04 | 0.07 / 0.06 |
| coin | Y45 | 256 | 10.1 | 7.04 | 1.4x | 0.13 / 0.04 | 0.11 / 0.06 |
| coin | X45Y30 | 256 | 10.7 | 6.81 | 1.6x | 0.12 / 0.05 | 0.06 / 0.06 |
| coin | X60Z30 | 256 | 9.70 | 6.22 | 1.6x | 0.11 / 0.05 | 0.05 / 0.05 |

## Private coin file (Earth Focus Many Vertical.3mf)

Each object at the pose it has in the file. Only the file name, counts and timings are recorded here; none of its geometry, outputs or corpus is stored.

- **JS:** cannot load (V8 string limit, 633 MB model XML). `readThreeMFParts` decodes the whole model XML into one string, which V8 caps near 512 MB (`threemf.js` `TextDecoder`), so the JS engine cannot open this file at all and no JS time exists.
- **Rust:** load (read + unzip + scan the whole file, median of 5) 8444 ms; 10 objects, 7723400 triangles in all; weld and build are per object, below.
- **Peak memory (Rust, whole `bench_m3` process, `/usr/bin/time -l`):** 6034 MiB. The stress models add a few MB; the coin file dominates.

| model | pose | triangles | JS ms | Rust ms | ratio | JS load / weld ms | Rust load / weld ms |
|---|---|---:|---:|---:|---:|---:|---:|
| Earth Focus Many Vertical | object 1 | 772340 | cannot load (V8 string limit, 633 MB model XML) | 959 | - | - | - / 224 |
| Earth Focus Many Vertical | object 2 | 772340 | cannot load (V8 string limit, 633 MB model XML) | 934 | - | - | - / 224 |
| Earth Focus Many Vertical | object 3 | 772340 | cannot load (V8 string limit, 633 MB model XML) | 940 | - | - | - / 228 |
| Earth Focus Many Vertical | object 4 | 772340 | cannot load (V8 string limit, 633 MB model XML) | 923 | - | - | - / 217 |
| Earth Focus Many Vertical | object 5 | 772340 | cannot load (V8 string limit, 633 MB model XML) | 933 | - | - | - / 218 |
| Earth Focus Many Vertical | object 6 | 772340 | cannot load (V8 string limit, 633 MB model XML) | 942 | - | - | - / 223 |
| Earth Focus Many Vertical | object 7 | 772340 | cannot load (V8 string limit, 633 MB model XML) | 933 | - | - | - / 211 |
| Earth Focus Many Vertical | object 8 | 772340 | cannot load (V8 string limit, 633 MB model XML) | 927 | - | - | - / 209 |
| Earth Focus Many Vertical | object 9 | 772340 | cannot load (V8 string limit, 633 MB model XML) | 936 | - | - | - / 219 |
| Earth Focus Many Vertical | object 10 | 772340 | cannot load (V8 string limit, 633 MB model XML) | 940 | - | - | - / 211 |
