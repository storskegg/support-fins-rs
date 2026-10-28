# Bench: every bench-* timing. They take the machine lock and record the machine load.
# New recipes of this area go here; the root justfile only holds shared variables and the imports.

# M3 timing, JS engine (Deno) vs single-threaded Rust engine (release), written to docs/PERF-M3.md. Skips private-fixtures/ cases when the file is absent.
bench-m3:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -q -p sf-testkit --example bench_m3
    # the machine lock (one benchmark or app test at a time) and the load figures wrap the whole run
    tools/lock/lock.py --bench bench-m3 -- bash -ec '
        deno run --allow-read --allow-write --allow-run --allow-env bench/m3.js run
        # peak memory of the Rust process (the private coin file dominates it when present)
        /usr/bin/time -l target/release/examples/bench_m3 2> target/bench-m3/rust-time.txt
        deno run --allow-read --allow-write --allow-run --allow-env bench/m3.js report'


# M5 suggest timing: web suggestOrientations under Deno against the Rust serial, parallel and wide searches.
bench-suggest:
    cargo build --release -q -p sf-testkit --example bench_suggest
    tools/lock/lock.py --bench bench-suggest -- bash -ec 'deno run -A bench/suggest.js; target/release/examples/bench_suggest'

# M5 timing: the Rust fin build per case beside the M3 serial numbers, written into docs/PERF-M5.md (Rust only, no Deno). `just bench-m5 needle` times matching cases and prints only. Skips private-fixtures/ cases when the file is absent.
bench-m5 filter="":
    cargo build --release -q -p sf-testkit --example bench_m5
    tools/lock/lock.py --bench bench-m5 -- target/release/examples/bench_m5 {{filter}}

# M5 cancel latency: how long after the flag is raised a build returns (docs/PERF-M5.md, "Live drag"). Prints a table; `just bench-cancel needle` runs matching cases.
bench-cancel filter="":
    cargo build --release -q -p sf-testkit --example bench_cancel
    tools/lock/lock.py --bench bench-cancel -- target/release/examples/bench_cancel {{filter}}

# M5 load / weld timing: a generated 1.5M-face STL and OBJ (under target/bench-load/, never committed) and the private 3MF when present. Prints a table; `just bench-load stl` runs one input.
bench-load *inputs:
    cargo build --release -q -p sf-testkit --example bench_load
    tools/lock/lock.py --bench bench-load -- target/release/examples/bench_load {{inputs}}

# Headless frame-time benchmark of the viewer on a generated 1.5M-triangle scene with fins and tines (docs/PERF-M5.md, "Rendering"); pass flags after the recipe, e.g. `just bench-render --tris 2000000`.
bench-render *args: _bindings-present
    swift build -j {{xcode_jobs}} -c release --package-path app/SupportFinsViewer --product viewer-bench
    tools/lock/lock.py --bench bench-render -- swift run --skip-build -c release --package-path app/SupportFinsViewer viewer-bench {{args}}
