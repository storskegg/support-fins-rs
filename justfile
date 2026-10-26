# Every recipe runs from the workspace root.

default: check

# fmt + clippy + every test (unit, ported, parity). Must be green before a PR.
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    deno test --allow-read --allow-write --allow-run=git,zstd --allow-env parity/oracle/lib/

# Rust parity replay only.
parity:
    cargo test -p parity --tests
    cargo test --workspace --test 'parity_*'

# Re-record oracle cases from the JS engine at UPSTREAM. `just oracle` = all, `just oracle jsnum` = one.
oracle module="":
    #!/usr/bin/env bash
    set -euo pipefail
    flags="--allow-read --allow-write --allow-run=git,zstd --allow-env"
    if [ -n "{{module}}" ]; then
        deno run $flags "parity/oracle/{{module}}.js"
    else
        for f in parity/oracle/*.js; do deno run $flags "$f"; done
    fi
