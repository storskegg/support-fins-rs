# App: the Swift bindings, the Xcode project and the macOS app tests (UI tests are the orchestrator's).
# New recipes of this area go here; the root justfile only holds shared variables and the imports.

# Build sf-ffi and generate the SupportFinsCore Swift package: the UniFFI Swift source and the
# XCFramework around the static library. Both are generated and gitignored, never committed: a fresh clone runs
# `just app-core app-project` (or any recipe that depends on them) before it can build the app.
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

# Generate the Swift bindings only when they are missing (a fresh clone), for recipes that need them but not a rebuild.
_bindings-present:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f app/SupportFinsCore/Sources/SupportFinsCore/sf_ffi.swift ] || [ ! -d app/SupportFinsCore/sf_ffiFFI.xcframework ]; then
        just swift-bindings
    fi

# Regenerate the Swift package and run its tests.
app-core: swift-bindings
    cd app/SupportFinsCore && swift test -j {{xcode_jobs}}

# Generate app/SupportFins.xcodeproj from app/project.yml (XcodeGen; the project is not committed).
app-project:
    xcodegen generate --spec app/project.yml --project app

# The viewer package's tests (camera maths, offscreen renders) and the app's tests.
app-test: app-core app-project
    tools/lock/lock.py app-test -- bash -ec "swift test -j {{xcode_jobs}} --package-path app/SupportFinsViewer; xcodebuild test -jobs {{xcode_jobs}} -project app/SupportFins.xcodeproj -scheme SupportFins -destination 'platform=macOS'"

# The window driven end to end (XCUITest): open a part, Add fins, change a setting, export. Needs the Accessibility
# permission for Xcode's test runner; `app-test` runs the same loop headless (EndToEndTests).
#
# The derived data path is fixed (see the top of the justfile), so the UI test runner's code signature stays the same
# from run to run and macOS does not ask for the Accessibility permission again. This takes over the mouse and
# keyboard: only the orchestrator runs it, never an agent and never `just gate`.
app-uitest: app-core app-project
    tools/lock/lock.py app-uitest -- xcodebuild test -jobs {{xcode_jobs}} -derivedDataPath "{{uitest_derived_data}}" -project app/SupportFins.xcodeproj -scheme SupportFinsUI -destination 'platform=macOS'
