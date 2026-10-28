# Engine: the Rust workspace -- format, lint, test, and the one gate a PR must pass.
# New recipes of this area go here; the root justfile only holds shared variables and the imports.

# fmt + clippy + every test (unit, ported, parity). Must be green before a PR.
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    deno test --allow-read --allow-write --allow-run=git,zstd --allow-env parity/oracle/lib/

# Every gate a PR needs, one summary table, non-zero exit on any failure: fmt, clippy, cargo test --workspace, the deno
# tests, cli-parity, and (when app/ or crates/sf-ffi changed vs origin/main) app-core, app-project, app-test. Run it
# before reporting. `--all` / `--no-app` / `--only fmt,clippy` to choose. Not app-uitest: only the orchestrator runs that.
gate *args:
    python3 tools/gate/gate.py {{args}}

# Build the OpenCascade 7.6.1 that STEP import links (static, ~30 min on 4 cores, once per machine; see crates/sf-step/README.md).
occt:
    tools/occt/build-occt.sh

# Rewrite parity/step-fixtures/*.step (the extra STEP inputs of the oracle) with OpenCascade; then `just oracle step`.
step-fixtures:
    #!/usr/bin/env bash
    set -euo pipefail
    occt="${SF_OCCT_DIR:-$HOME/.cache/support-fins/occt-7.6.1}"
    out="$(mktemp -d)/gen"
    c++ -std=c++17 -O1 -ffp-contract=off -I"$occt/include/opencascade" parity/step-fixtures/gen.cpp -o "$out" \
        -L"$occt/lib" $(ls "$occt"/lib/libTK*.a | sed 's|.*/lib\(TK[A-Za-z0-9]*\)\.a|-l\1|')
    "$out" parity/step-fixtures
