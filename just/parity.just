# Parity: the oracle recordings from the JS engine and the Rust-vs-JS comparisons.
# New recipes of this area go here; the root justfile only holds shared variables and the imports.

# Rust parity replay only.
parity:
    cargo test -p parity --tests
    cargo test --workspace --test 'parity_*'

# Re-record oracle cases from the JS engine at UPSTREAM. `just oracle` = all, `just oracle jsnum` = one.
oracle module="":
    #!/usr/bin/env bash
    set -euo pipefail
    flags="--allow-read --allow-write --allow-run=git,zstd --allow-env --node-modules-dir=none"
    if [ -n "{{module}}" ]; then
        deno run $flags "parity/oracle/{{module}}.js"
    else
        for f in parity/oracle/*.js; do deno run $flags "$f"; done
    fi

# Run the original JS tests at the pinned commit: `just js-test [file.test.js]`; refuses unless the checkout is at UPSTREAM.
js-test file="":
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(sed -n 's/^checkout=//p' UPSTREAM)"
    root="${root/#\~/$HOME}"
    want="$(sed -n 's/^commit=//p' UPSTREAM)"
    have="$(git -C "$root" rev-parse HEAD)"
    if [ "$have" != "$want" ]; then
        echo "js-test: $root is at $have but UPSTREAM pins $want; refusing to run." >&2
        exit 1
    fi
    cd "$root"
    if [ -n "{{file}}" ]; then
        deno test -A "tests/{{file}}"
    else
        deno test -A tests/
    fi

# The `support-fins` binary against the JS command line on the same inputs: exit code, stdout, stderr and every output file, byte for byte.
cli-parity filter="":
    cargo build --release -p sf-cli
    deno run --allow-read --allow-write --allow-run --allow-env parity/cli/cli_parity.js target/release/support-fins {{filter}}
