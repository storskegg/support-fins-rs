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

# M3 timing, JS engine (Deno) vs single-threaded Rust engine (release), written to docs/PERF-M3.md. Skips private-fixtures/ cases when the file is absent.
bench-m3:
    #!/usr/bin/env bash
    set -euo pipefail
    deno run --allow-read --allow-write --allow-run --allow-env bench/m3.js run
    cargo build --release -q -p sf-testkit --example bench_m3
    # peak memory of the Rust process (the private coin file dominates it when present)
    /usr/bin/time -l target/release/examples/bench_m3 2> target/bench-m3/rust-time.txt
    deno run --allow-read --allow-write --allow-run --allow-env bench/m3.js report

# Build sf-ffi and generate the SupportFinsCore Swift package: the UniFFI Swift source (committed) and the
# XCFramework around the static library (gitignored, rebuilt here).
swift-bindings:
    #!/usr/bin/env bash
    set -euo pipefail
    # the app targets macOS 26: build the Rust and the Manifold C++ for it, not for the SDK's newer default
    export MACOSX_DEPLOYMENT_TARGET=26.0
    cargo build --release -p sf-ffi --target aarch64-apple-darwin
    lib=target/aarch64-apple-darwin/release/libsf_ffi.a
    pkg=app/SupportFinsCore
    gen=target/swift-gen
    rm -rf "$gen" && mkdir -p "$gen/headers"
    cargo run --release -q -p sf-ffi --bin uniffi-bindgen -- generate --library "$lib" --language swift --out-dir "$gen"
    cp "$gen/sf_ffi.swift" "$pkg/Sources/SupportFinsCore/sf_ffi.swift"
    cp "$gen/sf_ffiFFI.h" "$gen/headers/"
    cp "$gen/sf_ffiFFI.modulemap" "$gen/headers/module.modulemap"
    rm -rf "$pkg/sf_ffiFFI.xcframework"
    xcodebuild -create-xcframework -library "$lib" -headers "$gen/headers" -output "$pkg/sf_ffiFFI.xcframework"

# Regenerate the Swift package and run its tests.
app-core: swift-bindings
    cd app/SupportFinsCore && swift test

# Generate app/SupportFins.xcodeproj from app/project.yml (XcodeGen; the project is not committed).
app-project:
    xcodegen generate --spec app/project.yml --project app

# The viewer package's tests (camera maths, offscreen renders) and the app's tests.
app-test: app-core app-project
    swift test --package-path app/SupportFinsViewer
    xcodebuild test -project app/SupportFins.xcodeproj -scheme SupportFins -destination 'platform=macOS'

# The window driven end to end (XCUITest): open a part, Add fins, change a setting, export. Needs the Accessibility
# permission for Xcode's test runner; `app-test` runs the same loop headless (EndToEndTests).
app-uitest: app-core app-project
    xcodebuild test -project app/SupportFins.xcodeproj -scheme SupportFinsUI -destination 'platform=macOS'

# M5 load / weld timing: a generated 1.5M-face STL and OBJ (under target/bench-load/, never committed) and the private 3MF when present. Prints a table; `just bench-load stl` runs one input.
bench-load *inputs:
    cargo build --release -q -p sf-testkit --example bench_load
    target/release/examples/bench_load {{inputs}}
