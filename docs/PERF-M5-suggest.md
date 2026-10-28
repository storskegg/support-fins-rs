# Suggest orientation timings (M5, track D)

Wall time of `suggestOrientations` (`top: 3`, `threshold: 45`, default config; the call the parity corpus records) on this Mac
(Apple M2 Max, 12 cores), measured by `just bench-suggest`: `bench/suggest.js` times the web's function under Deno at the
pin, `crates/sf-testkit/examples/bench_suggest.rs` times the Rust engine on the same files. Median of 5 after one
discarded warm-up. The parallel rankings are asserted equal to the one-thread rankings in the bench itself, and the default
path is byte-identical to the parity corpus (`parity_orient`).

- **JS**: the web's search (its candidate set: "web cands").
- **Rust serial**: the same candidate set on a one-thread Rayon pool.
- **Rust parallel**: the same candidate set on the global pool (the default `suggest_orientations`).
- **wide**: `suggest_orientations_wide(..., density = 10)`: about 10x the candidates, same scoring and wording.
- The machine was shared with three other agents (load average 15-25) during this run, so absolute numbers are noisy and
  the ratios are the thing to read; the orchestrator re-records in a quiet run.

## Where the time goes (profile)

An in-code timer around the two passes (one thread, temporary, not committed):

| model | pass 1: `analyze` on every candidate | pass 2: `build_fins` on the 5 finalists |
|---|---:|---:|
| needle (17 candidates) | 0.1 ms | 72 ms |
| needle, wide (150 candidates) | 0.9 ms | 120 ms |
| torus (14 candidates) | 0.4 ms | 49 ms |
| tall-coupon (12 candidates, 17k triangles) | 5 ms | 4526 ms |

The ranking is over 99.9% finalist builds; scoring candidates is cheap. That is why the wide search costs about what the
web search costs: it analyses 10x as many poses but builds the same five finalists. It also means the parallel speed-up is
bounded by the slowest single finalist build (about 3-4x on 12 cores for the large coupons), and only `build_fins` itself
(track C) can lower that.

## Method

Pass 1 (one `analyze` per candidate) and pass 2 (one `build_fins` per finalist) are Rayon `par_iter` over the candidate list
with an indexed collect, so the stable sorts and the tie-breaks see the serial sequence. A pass-1 result keeps no face flags
except the last candidate's (the one the web's aliasing reuses for every finalist), so 180 candidates do not hold 180 flag
sets. The cancel flag is polled before each candidate and each finalist.

## Results

| model | triangles | web cands | JS ms | Rust serial ms | Rust parallel ms | wide cands | wide ms | JS / wide |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| arch | 20 | 8 | 7.06 | 2.07 | 2.34 | 8 | 2.33 | 3.0x |
| bar | 12 | 6 | 0.16 | 0.02 | 0.09 | 6 | 0.10 | 1.6x |
| coin | 256 | 16 | 95.6 | 41.5 | 15.4 | 70 | 15.4 | 6.2x |
| cone | 96 | 17 | 44.9 | 22.1 | 8.24 | 54 | 8.15 | 5.5x |
| cube | 12 | 6 | 0.07 | 0.02 | 0.08 | 6 | 0.11 | 0.6x |
| cylinder | 192 | 16 | 30.3 | 17.6 | 7.73 | 54 | 11.9 | 2.5x |
| hexprism | 20 | 10 | 0.34 | 0.11 | 0.24 | 10 | 0.21 | 1.6x |
| lbracket | 20 | 6 | 44.1 | 21.5 | 22.1 | 6 | 22.0 | 2.0x |
| lowledge | 20 | 6 | 23.3 | 9.58 | 9.74 | 6 | 9.70 | 2.4x |
| needle | 384 | 17 | 138 | 72.2 | 19.8 | 150 | 38.0 | 3.6x |
| plate | 12 | 6 | 0.04 | 0.02 | 0.10 | 6 | 0.08 | 0.5x |
| plus | 44 | 6 | 73.7 | 36.3 | 13.6 | 6 | 13.7 | 5.4x |
| portal | 32 | 6 | 56.7 | 34.3 | 10.4 | 6 | 10.5 | 5.4x |
| pyramid | 8 | 10 | 0.07 | 0.02 | 0.10 | 10 | 0.09 | 0.8x |
| ramp | 16 | 7 | 0.18 | 0.03 | 0.08 | 7 | 0.09 | 2.0x |
| sphere | 1280 | 18 | 50.6 | 30.0 | 9.48 | 180 | 9.13 | 5.5x |
| staircase | 28 | 6 | 26.0 | 12.6 | 12.8 | 6 | 12.8 | 2.0x |
| torus | 2048 | 14 | 83.9 | 49.5 | 19.6 | 180 | 27.7 | 3.0x |
| tshape | 28 | 6 | 58.3 | 28.3 | 14.6 | 6 | 14.9 | 3.9x |
| tube | 384 | 14 | 107 | 63.6 | 22.5 | 54 | 22.3 | 4.8x |
| ushape | 28 | 6 | 49.0 | 23.8 | 14.0 | 6 | 14.3 | 3.4x |
| wedge | 8 | 7 | 0.17 | 0.05 | 0.15 | 7 | 0.12 | 1.4x |
| cutout-coupon | 16766 | 12 | 1394 | 708 | 309 | 95 | 96.3 | 14.5x |
| honeycomb-coupon | 18460 | 12 | 977 | 522 | 267 | 109 | 75.2 | 13.0x |
| sampler-coupon | 15538 | 12 | 20584 | 6487 | 2350 | 162 | 308 | 66.8x |
| tall-coupon | 17498 | 12 | 13684 | 4786 | 1311 | 115 | 1149 | 11.9x |
| orient-coupon | 9614 | 12 | 7803 | 3951 | 660 | 108 | 288 | 27.1x |
| bore-coupon | 9448 | 12 | 3476 | 879 | 211 | 139 | 511 | 6.8x |
| torus_flat | 6144 | 14 | 629 | 237 | 49.3 | 156 | 67.1 | 9.4x |

Notes:

- "wide ms" is below "Rust parallel ms" on the large coupons because the wider pass 1 changes which five poses are the
  finalists (cheaper ones to build); the scores are the web's.
- A model with few distinct face normals (a box, an L-bracket, a plus) has no more stable down-faces than the web already
  tries: the finer step finds nothing new, so its wide candidate count stays at the web's (6-10). The 10x target is met
  by curved and many-faceted parts (sphere 18 to 180, torus 14 to 156, the coupons 12 to 95-162), not by boxes.
- The web's candidate count in the table is computed by the bench (a verbatim copy of `candidateDowns`) and asserted equal to
  Rust's `candidate_downs`.

Private coin file (`Earth Focus Many Vertical.3mf`, object 1, 772,340 triangles; the JS engine cannot load the file, so Rust only):

| web cands | Rust serial ms | Rust parallel ms | wide cands | wide ms |
|---:|---:|---:|---:|---:|
| 14 | 3626 | 1428 | 134 | 1340 |
