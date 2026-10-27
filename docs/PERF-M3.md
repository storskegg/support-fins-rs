# M3 timing: JS engine vs single-threaded Rust engine

The serial baseline M5 improves on. Regenerate with `just bench-m3` (writes this file).

- **Machine:** Apple M2 Max, macOS 26.6.2.
- **JS:** Deno 2.9.8, `buildFins` from the pinned upstream (299ef49). **Rust:** rustc 1.99.1 (58cf8f9c3 2026-10-13), `cargo build --release`, `build_fins`, one thread.
- **Builds:** the site's own request (`tests/_scene.js` `siteOpts()`: Auto, PLA, tines, bed pad, the form untouched), threshold 45.
- **Method:** each case is timed 5 times after one discarded warm-up; the table gives the median. The phases are separate:
  **load** = read the file and parse it into a position soup, **weld** = `buildTopology` + `analyze`, and the **JS ms / Rust ms** columns are `buildFins` alone (the ratio is JS over Rust, so above 1 means Rust is faster).
  Every run starts from a fresh topology, so lazily built grids are charged to the build.
- **Same work:** the Rust build produced the same support vertex count as JS in 77 of 77 cases. Both engines are tested for exact equality in the parity corpus, so these timings compare equal work.

## Summary

- Stress models at their golden poses: 48 builds, total JS 1252 ms vs Rust 596 ms (2.1x overall, median per-build ratio 2.4x)
- Worst orientations of the torus, sphere, needle, tube, coin models: 29 builds, total JS 1271 ms vs Rust 695 ms (1.8x overall, median per-build ratio 1.8x)
- Private coin file: not present on this machine (skipped).

## Stress models at their golden poses

Every stress model at the poses of its non-draw, non-sway `tests/golden` scenes ("up" = as modelled, `X40` = tilted 40 degrees), at default settings.

| model | pose | triangles | JS ms | Rust ms | ratio | JS load / weld ms | Rust load / weld ms |
|---|---|---:|---:|---:|---:|---:|---:|
| cube | X20 | 12 | 38.8 | 13.1 | 3.0x | 0.20 / 0.07 | 0.05 / 0.01 |
| cube | X30 | 12 | 28.9 | 11.9 | 2.4x | 0.20 / 0.05 | 0.04 / 0.00 |
| cube | X40 | 12 | 25.9 | 11.2 | 2.3x | 0.19 / 0.05 | 0.04 / 0.00 |
| lbracket | X35 | 20 | 31.5 | 13.3 | 2.4x | 0.16 / 0.06 | 0.05 / 0.01 |
| sphere | up | 1280 | 21.0 | 10.2 | 2.1x | 0.28 / 0.29 | 0.05 / 0.34 |
| staircase | X40 | 28 | 45.5 | 21.3 | 2.1x | 0.20 / 0.02 | 0.07 / 0.01 |
| arch | up | 20 | 0.07 | 0.01 | 11.4x | 0.05 / 0.01 | 0.03 / 0.00 |
| arch | X40 | 20 | 31.3 | 14.7 | 2.1x | 0.20 / 0.02 | 0.05 / 0.01 |
| bar | up | 12 | 0.04 | 0.00 | 8.7x | 0.03 / 0.01 | 0.03 / 0.00 |
| bar | X40 | 12 | 9.60 | 4.38 | 2.2x | 0.12 / 0.02 | 0.03 / 0.00 |
| coin | up | 256 | 0.54 | 0.03 | 20.0x | 0.07 / 0.07 | 0.03 / 0.04 |
| coin | X40 | 256 | 71.1 | 36.3 | 2.0x | 0.22 / 0.07 | 0.05 / 0.05 |
| cone | up | 96 | 0.07 | 0.01 | 4.9x | 0.04 / 0.02 | 0.03 / 0.02 |
| cone | X40 | 96 | 121 | 60.1 | 2.0x | 0.21 / 0.04 | 0.07 / 0.02 |
| cube | up | 12 | 0.02 | 0.00 | 4.4x | 0.05 / 0.01 | 0.03 / 0.00 |
| cylinder | up | 192 | 0.20 | 0.02 | 9.3x | 0.05 / 0.04 | 0.03 / 0.03 |
| cylinder | X40 | 192 | 14.5 | 7.76 | 1.9x | 0.16 / 0.05 | 0.04 / 0.04 |
| hexprism | up | 20 | 0.02 | 0.01 | 3.9x | 0.04 / 0.01 | 0.02 / 0.00 |
| hexprism | X40 | 20 | 16.4 | 6.91 | 2.4x | 0.17 / 0.02 | 0.03 / 0.01 |
| lbracket | up | 20 | 0.03 | 0.01 | 5.3x | 0.04 / 0.01 | 0.02 / 0.00 |
| lbracket | X40 | 20 | 69.7 | 31.1 | 2.2x | 0.17 / 0.02 | 0.06 / 0.01 |
| lowledge | up | 20 | 82.3 | 35.9 | 2.3x | 0.20 / 0.02 | 0.05 / 0.01 |
| lowledge | X40 | 20 | 17.7 | 7.43 | 2.4x | 0.21 / 0.03 | 0.04 / 0.01 |
| needle | up | 384 | 0.73 | 0.05 | 16.0x | 0.05 / 0.07 | 0.03 / 0.06 |
| needle | X40 | 384 | 94.7 | 48.8 | 1.9x | 0.20 / 0.10 | 0.09 / 0.08 |
| plate | up | 12 | 0.02 | 0.00 | 3.7x | 0.03 / 0.01 | 0.03 / 0.00 |
| plate | X40 | 12 | 53.9 | 26.6 | 2.0x | 0.18 / 0.02 | 0.08 / 0.01 |
| plus | up | 44 | 0.03 | 0.01 | 3.3x | 0.03 / 0.01 | 0.03 / 0.01 |
| plus | X40 | 44 | 14.2 | 7.14 | 2.0x | 0.15 / 0.02 | 0.04 / 0.02 |
| portal | up | 32 | 0.03 | 0.01 | 4.0x | 0.03 / 0.01 | 0.03 / 0.01 |
| portal | X40 | 32 | 33.7 | 15.8 | 2.1x | 0.17 / 0.02 | 0.04 / 0.01 |
| pyramid | up | 8 | 0.02 | 0.00 | 5.4x | 0.03 / 0.01 | 0.02 / 0.00 |
| pyramid | X40 | 8 | 7.06 | 3.50 | 2.0x | 0.11 / 0.01 | 0.03 / 0.00 |
| ramp | up | 16 | 0.02 | 0.01 | 4.4x | 0.03 / 0.01 | 0.02 / 0.00 |
| ramp | X40 | 16 | 50.6 | 24.5 | 2.1x | 0.19 / 0.02 | 0.04 / 0.01 |
| sphere | X40 | 1280 | 23.6 | 11.0 | 2.1x | 0.20 / 0.21 | 0.05 / 0.33 |
| staircase | up | 28 | 0.04 | 0.01 | 5.7x | 0.10 / 0.01 | 0.02 / 0.01 |
| torus | up | 2048 | 46.3 | 23.7 | 2.0x | 0.24 / 0.28 | 0.05 / 0.38 |
| torus | X40 | 2048 | 60.6 | 31.6 | 1.9x | 0.23 / 0.24 | 0.05 / 0.39 |
| tshape | up | 28 | 0.02 | 0.01 | 3.7x | 0.03 / 0.01 | 0.02 / 0.01 |
| tshape | X40 | 28 | 37.5 | 17.2 | 2.2x | 0.16 / 0.02 | 0.04 / 0.01 |
| tube | up | 384 | 0.35 | 0.04 | 8.9x | 0.04 / 0.05 | 0.03 / 0.06 |
| tube | X40 | 384 | 25.6 | 12.1 | 2.1x | 0.18 / 0.07 | 0.04 / 0.07 |
| ushape | up | 28 | 0.02 | 0.01 | 3.2x | 0.04 / 0.01 | 0.03 / 0.01 |
| ushape | X40 | 28 | 83.9 | 39.1 | 2.1x | 0.17 / 0.02 | 0.05 / 0.01 |
| wedge | up | 8 | 0.02 | 0.00 | 3.8x | 0.03 / 0.01 | 0.03 / 0.00 |
| wedge | X40 | 8 | 33.1 | 16.0 | 2.1x | 0.16 / 0.01 | 0.04 / 0.00 |
| torus | X30 | 2048 | 59.7 | 33.2 | 1.8x | 0.23 / 0.24 | 0.05 / 0.39 |

## Worst orientations

The five stress models with the most triangles (torus, sphere, needle, tube, coin) in each pose of `prototype/stress/run.js` (flat, X30, X45, X60, Y45, X45Y30, X60Z30), other than the ones above.

| model | pose | triangles | JS ms | Rust ms | ratio | JS load / weld ms | Rust load / weld ms |
|---|---|---:|---:|---:|---:|---:|---:|
| torus | X45 | 2048 | 96.5 | 49.1 | 2.0x | 0.23 / 0.25 | 0.06 / 0.40 |
| torus | X60 | 2048 | 58.4 | 28.6 | 2.0x | 0.26 / 0.25 | 0.07 / 0.41 |
| torus | Y45 | 2048 | 83.6 | 46.5 | 1.8x | 0.22 / 0.24 | 0.09 / 0.40 |
| torus | X45Y30 | 2048 | 74.3 | 39.6 | 1.9x | 0.24 / 0.25 | 0.09 / 0.40 |
| torus | X60Z30 | 2048 | 56.9 | 27.9 | 2.0x | 0.22 / 0.24 | 0.07 / 0.39 |
| sphere | X30 | 1280 | 19.1 | 11.0 | 1.7x | 0.21 / 0.19 | 0.05 / 0.33 |
| sphere | X45 | 1280 | 23.8 | 14.1 | 1.7x | 0.22 / 0.19 | 0.06 / 0.33 |
| sphere | X60 | 1280 | 19.1 | 11.7 | 1.6x | 0.19 / 0.19 | 0.07 / 0.33 |
| sphere | Y45 | 1280 | 23.9 | 14.6 | 1.6x | 0.20 / 0.19 | 0.07 / 0.33 |
| sphere | X45Y30 | 1280 | 19.3 | 11.7 | 1.7x | 0.20 / 0.20 | 0.06 / 0.33 |
| sphere | X60Z30 | 1280 | 19.3 | 12.1 | 1.6x | 0.16 / 0.20 | 0.06 / 0.33 |
| needle | X30 | 384 | 93.6 | 50.7 | 1.8x | 0.19 / 0.06 | 0.08 / 0.08 |
| needle | X45 | 384 | 6.37 | 3.06 | 2.1x | 0.09 / 0.06 | 0.04 / 0.07 |
| needle | X60 | 384 | 115 | 63.9 | 1.8x | 0.22 / 0.06 | 0.06 / 0.08 |
| needle | Y45 | 384 | 5.45 | 3.06 | 1.8x | 0.09 / 0.06 | 0.04 / 0.07 |
| needle | X45Y30 | 384 | 47.3 | 23.5 | 2.0x | 0.20 / 0.06 | 0.07 / 0.08 |
| needle | X60Z30 | 384 | 117 | 65.5 | 1.8x | 0.22 / 0.07 | 0.08 / 0.08 |
| tube | X30 | 384 | 26.3 | 14.4 | 1.8x | 0.14 / 0.06 | 0.05 / 0.07 |
| tube | X45 | 384 | 10.7 | 5.90 | 1.8x | 0.12 / 0.05 | 0.04 / 0.07 |
| tube | X60 | 384 | 65.5 | 36.2 | 1.8x | 0.17 / 0.06 | 0.11 / 0.08 |
| tube | Y45 | 384 | 10.7 | 5.93 | 1.8x | 0.12 / 0.05 | 0.04 / 0.07 |
| tube | X45Y30 | 384 | 60.1 | 34.6 | 1.7x | 0.19 / 0.06 | 0.06 / 0.08 |
| tube | X60Z30 | 384 | 88.2 | 52.1 | 1.7x | 0.18 / 0.06 | 0.10 / 0.08 |
| coin | X30 | 256 | 69.0 | 36.5 | 1.9x | 0.17 / 0.05 | 0.06 / 0.05 |
| coin | X45 | 256 | 11.0 | 6.61 | 1.7x | 0.15 / 0.05 | 0.04 / 0.05 |
| coin | X60 | 256 | 19.8 | 6.92 | 2.9x | 0.14 / 0.04 | 0.04 / 0.05 |
| coin | Y45 | 256 | 10.7 | 6.78 | 1.6x | 0.13 / 0.05 | 0.03 / 0.05 |
| coin | X45Y30 | 256 | 10.8 | 6.41 | 1.7x | 0.13 / 0.04 | 0.04 / 0.05 |
| coin | X60Z30 | 256 | 9.53 | 5.96 | 1.6x | 0.10 / 0.04 | 0.04 / 0.05 |
