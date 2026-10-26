# Every recipe runs from the workspace root.

default: check

# fmt + clippy + every test (unit, ported, parity). Must be green before a PR.
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

# Rust parity replay only.
parity:
    cargo test -p parity --tests
    cargo test --workspace --test 'parity_*'

# Re-record oracle cases from the JS engine at UPSTREAM. `just oracle` = all, `just oracle jsnum` = one.
oracle module="":
    @echo "not yet implemented (M0 track C)"; exit 1
