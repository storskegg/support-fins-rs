# Every recipe runs from the workspace root. This file only holds what every area shares; the recipes live in
# just/*.just, one file per area: engine (build, check, gate), parity (oracle and the JS/Rust comparisons),
# app (Swift bindings, Xcode project, app tests), bench (the bench-* timings) and tools (dashboard, tidy).
# NEW RECIPES GO IN THEIR AREA'S FILE, not here. `just --list` shows all of them.
#
# app-test, app-uitest and every bench-* take the machine-wide lock (tools/lock): one at a time on this Mac,
# whichever worktree they run from. Benchmarks also record the machine load before and during the run.
#
# Per-job build caps. Up to 4 agents build at once on a 12-core Mac; each at 4 jobs is ~16
# compile threads, which keeps the load near the core count where uncapped builds (12 jobs each)
# sent it to 30-70. Override per run: `CARGO_BUILD_JOBS=8 just check`, `XCODE_JOBS=8 just app-test`.
export CARGO_BUILD_JOBS := env_var_or_default("CARGO_BUILD_JOBS", "4")
xcode_jobs := env_var_or_default("XCODE_JOBS", "4")
# One derived data path for every worktree, outside them (see app-uitest).
uitest_derived_data := env_var_or_default("UITEST_DERIVED_DATA", env_var("HOME") + "/RustRoverProjects/sfrs-wt/uitest.dd")

import 'just/engine.just'
import 'just/parity.just'
import 'just/app.just'
import 'just/bench.just'
import 'just/tools.just'

default: check
