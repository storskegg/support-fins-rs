# Load and weld timings (M5, track B)

How long it takes to get a file from disk to a `Topology` and its first `analyze`, measured by `just bench-load`
(`crates/sf-testkit/examples/bench_load.rs`) on this Mac (Apple M2 Max, 12 cores, macOS 26.6). The 1.5M-face inputs are
generated at bench time under `target/bench-load/` (a torus, 1000 x 750 quads, coordinates rounded to f32 like an STL; never
committed). The private 3MF (`private-fixtures/Earth Focus Many Vertical.3mf`: 10 objects x 772,340 triangles, 117 MB, one
633 MB model part) is timed when present; only its counts and timings are recorded here.

- **read** = `fs::read` of the file (page-cached), **parse** = bytes to a position soup (`read_stl`, `read_obj`,
  `read_three_mf_unmerged`, which is what the app calls), **weld** = `build_topology`, **analyze** = the first `analyze`
  on that topology (identity pose, 45 degrees). Median of 5 runs after one warm-up (the private file: parse median of 3;
  weld and analyze are the mean over its 10 objects of the median of 3).
- Every row also prints a digest of the topology (vertex ids, adjacency, normals, areas) and the analysis flags. The digest is
  identical before and after, per input, so these timings compare equal output.
- The machine was shared with three other agents (load average 12-22) during these runs: read the ratios, and expect the
  orchestrator's quiet run to be a little better.

## Results

Before = `main` at cc3b2ad (serial weld, line-by-line OBJ reader, string-based 3MF scan). After = the four load PRs together
(#138 parallel weld and analyze, #141 OBJ reader, #147 3MF scan, #148 inflate through zlib-rs).

| input | triangles | parse before ms | parse after ms | weld before ms | weld after ms | analyze before ms | analyze after ms |
|---|---:|---:|---:|---:|---:|---:|---:|
| binary STL | 1,500,000 | 7.7 | 7.6 | 478-522 | 42-51 | 23 | 12.5 |
| OBJ (750k vertices) | 1,500,000 | 434 | 35-40 | 499 | 43-46 | 24 | 13 |
| private 3MF, whole file | 7,723,400 | 8300-8500 | 1080-1100 | 217 per object | 24 per object | 13 per object | 6.6 per object |

Reading a 1.5M-face STL and welding it takes about 60 ms of CPU-bound work on top of the file read; up to the first analysis,
about 75 ms (was 535 ms).

Peak resident memory of loading the private 3MF (`/usr/bin/time -l`, one `read_three_mf_unmerged` call): 3.39 GB before,
1.95 GB after (`read_three_mf`, which still builds the merged soup, peaks at 2.39 GB).

## Where the time went, and what changed

- **Weld (480 -> 45 ms).** The serial weld spent its time in 4.5M random probes of a 64 MB slot table plus a 108 MB key
  array. The parallel weld (`weld.rs`) groups equal corners by hash bucket on all cores and recovers the serial id order from
  each group's first corner, so the topology is identical. Phases at 1.5M faces: normals and hashes 11 ms, vertex grouping
  28, id ranking 6, edge grouping 24, pair collection 13.
- **First analyze (23 -> 13 ms).** The rotation and bounds pass and the per-face flags run on all cores; the area sums,
  union-find and region build stay serial (11 ms).
- **OBJ (434 -> 37 ms).** Lines are cut into chunks scanned in parallel, plain decimals go through Rust's float parse, face
  indices are resolved against the vertex count before each line and written straight into the object's soup. Before: per-line
  `Vec`s, `string_to_number`, and an f32 round trip per coordinate into a 108 MB output.
- **3MF (8.4 s -> 1.1 s).** The model XML parse took 7.3 of the 8.3 s: a full-string copy, an owned attribute map per element
  (about 130M allocations), per-triangle pushes. It is now a zero-allocation scanner over pieces cut at element starts and
  scanned in parallel (129 ms for the 633 MB part), replayed in document order (29 ms), then one `emit_object` per object
  (about 13 ms each). What is left is the inflate of the single 633 MB deflate stream: about 0.75 s with zlib-rs (1.05 s
  with miniz_oxide), serial because one deflate stream cannot be split.

## Tried and not worth it

- A larger output buffer for the miniz_oxide streaming inflate (64 KB, 1 MB, 4 MB): no change. miniz_oxide's one-shot
  `decompress_to_vec` is 2x slower than the streaming loop already in `unzip`.
- Overlapping the inflate with the XML scan (feed pieces to the scanner as the inflate produces them): the scan is 130 ms of
  parallel work against 750 ms of inflate, so the ceiling is about 10% of the load, for a streaming reader that has to carry
  elements across buffer boundaries. Not built.
- libdeflate (C) inflates the plate in about 290 ms against zlib-rs' 485 and miniz_oxide's 925 (standalone harness, quiet
  machine), but its safe Rust wrapper cannot tell how many input bytes the stream used, so it cannot reproduce the "bytes after
  the end of the stream" error without an `unsafe` FFI call. Not done.
- Parallel `emit_object` across build items: the items share the painter, the lazily parsed parts and the role arena, and each
  takes about 13 ms. Not done.

## Method notes

Parallel results are merged in the serial order (index-ordered collects, atomic writes to distinct slots, left-to-right
reductions); nothing is pushed from two threads. `tests/weld_parallel.rs` (topology and analysis), `tests/obj_fast.rs`
(OBJ reader against the line-by-line port) and `tests/threemf_chunked.rs` (chunked model parse against the sequential one)
compare the paths bit for bit. Loading is not cancellable through the generation counter (it never was: it is one blocking
FFI call); the parallel passes are short and run on the global rayon pool.
