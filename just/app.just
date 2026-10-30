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
    # --features app-alloc: BigBlocks is the app's global allocator (C-05)
    cargo build --release -p sf-ffi --features app-alloc --target aarch64-apple-darwin
    lib=target/aarch64-apple-darwin/release/libsf_ffi.a
    pkg=app/SupportFinsCore
    gen=target/swift-gen
    rm -rf "$gen" && mkdir -p "$gen/headers"
    cargo run --release -q -p sf-bindgen --bin uniffi-bindgen -- generate --library "$lib" --language swift --out-dir "$gen"
    cp "$gen/sf_ffi.swift" "$pkg/Sources/SupportFinsCore/sf_ffi.swift"
    # FfiError's errorDescription is UniFFI's String(reflecting: self), a Swift type dump; use the hand-written one
    # (FfiError+Description.swift). Fail loudly if a UniFFI update changes the line.
    grep -c 'String(reflecting: self)' "$pkg/Sources/SupportFinsCore/sf_ffi.swift" | grep -qx 1
    sed -i '' 's/String(reflecting: self)/ffiErrorDescription(self)/' "$pkg/Sources/SupportFinsCore/sf_ffi.swift"
    cp "$gen/sf_ffiFFI.h" "$gen/headers/"
    cp "$gen/sf_ffiFFI.modulemap" "$gen/headers/module.modulemap"
    rm -rf "$pkg/sf_ffiFFI.xcframework"
    xcodebuild -create-xcframework -library "$lib" -headers "$gen/headers" -output "$pkg/sf_ffiFFI.xcframework"
    # The app never runs OpenCascade (X-01): sf-ffi is built without sf-step's `occt` feature (a separate cargo run, so
    # sf-step-xpc's features do not unify into it), and this fails if the kernel got into the app's library anyway.
    if nm -gU "$lib" 2>/dev/null | grep -q -e '_sf_step_read$' -e 'STEPControl_Reader'; then
        echo "error: $lib links OpenCascade; the app must read STEP through the StepReader XPC service" >&2
        exit 1
    fi
    # The StepReader XPC service's library (crates/sf-step-xpc): sf-step with OpenCascade, behind include/sf_step_xpc.h.
    cargo build --release -p sf-step-xpc --target aarch64-apple-darwin
    rm -rf app/StepRead/sf_step_xpc.xcframework
    xcodebuild -create-xcframework -library target/aarch64-apple-darwin/release/libsf_step_xpc.a \
        -headers crates/sf-step-xpc/include -output app/StepRead/sf_step_xpc.xcframework

# Generate the Swift bindings only when they are missing (a fresh clone), for recipes that need them but not a rebuild.
_bindings-present:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f app/SupportFinsCore/Sources/SupportFinsCore/sf_ffi.swift ] || [ ! -d app/SupportFinsCore/sf_ffiFFI.xcframework ] || [ ! -d app/StepRead/sf_step_xpc.xcframework ]; then
        just swift-bindings
    fi

# Regenerate the Swift package and run its tests.
app-core: swift-bindings
    cd app/SupportFinsCore && swift test -j {{xcode_jobs}}

# Generate app/SupportFins.xcodeproj from app/project.yml (XcodeGen; the project is not committed).
app-project:
    xcodegen generate --spec app/project.yml --project app

# The viewer package's tests (camera maths, offscreen renders) and the app's tests, then a check that no test crashed
# natively: a native crash dump written during the run fails the step and prints its stack (tools/gate/app_test.sh).
app-test: app-core app-project
    tools/lock/lock.py app-test -- tools/gate/app_test.sh {{xcode_jobs}}

# Nothing drives the mouse or keyboard, so any agent may run it, and `just gate` does whenever the app gates run: a UI
# test that does not compile cannot reach review. Its own derived data inside the worktree (`.dd-ui`, gitignored),
# apart from app-uitest's fixed path, so it never touches the signed runner the Accessibility permission was given to.
# Compile the UI tests without running them (xcodebuild build-for-testing).
app-uibuild: app-core app-project
    tools/lock/lock.py app-uibuild -- xcodebuild build-for-testing -jobs {{xcode_jobs}} -derivedDataPath "{{justfile_directory()}}/.dd-ui" -project app/SupportFins.xcodeproj -scheme SupportFinsUI -destination 'platform=macOS'

# The window driven end to end (XCUITest): open a part, Add fins, change a setting, export. Needs the Accessibility
# permission for Xcode's test runner; `app-test` runs the same loop headless (EndToEndTests).
#
# Every worktree shares the derived data path (below), so after a run from another worktree the precompiled modules built from its
# headers (sf_ffiFFI.h) fail the next run ("has been modified since the module file ... was built"). So, under the
# lock, each run first drops the module caches built from project headers (Swift and clang explicit modules, the
# implicit module cache) and rebuilds them from its own worktree. The SDK's modules (SDKExplicitPrecompiledModules)
# and the content-addressed CompilationCache stay.
#
# The derived data path is fixed (see the top of the justfile), so the UI test runner's code signature stays the same
# from run to run and macOS does not ask for the Accessibility permission again. This takes over the mouse and
# keyboard: only the orchestrator runs it, never an agent and never `just gate`.
app-uitest: app-core app-project
    tools/lock/lock.py app-uitest -- bash -ec 'dd="{{uitest_derived_data}}"; rm -rf "$dd/Build/Intermediates.noindex/SwiftExplicitPrecompiledModules" "$dd/Build/Intermediates.noindex/ExplicitPrecompiledModules" "$dd/ModuleCache.noindex"; xcodebuild test -jobs {{xcode_jobs}} -derivedDataPath "$dd" -project app/SupportFins.xcodeproj -scheme SupportFinsUI -destination "platform=macOS"'

# Languages (docs/UI-MAP.md "Languages"; tools/i18n/README.md): the String Catalog is generated from the web's translation
# tables, so re-run this at the upstream catch-up. `--check` fails when the file differs from what the tables generate.
i18n-gen *args:
    deno run -A --config tools/i18n/deno.json tools/i18n/gen.ts {{args}}

# The catalog is what the web's tables generate (cheap, no build): part of every `just gate`.
i18n-catalog:
    deno run -A --config tools/i18n/deno.json tools/i18n/gen.ts --check

# Every string the app shows is a web string (or listed in tools/i18n/native-only.json) and every web translation is in the
# catalog. Reads the strings the compiler extracted from the build of `app-uibuild` (`.dd-ui`), so it builds first.
i18n-check: app-uibuild
    deno run -A --config tools/i18n/deno.json tools/i18n/parity.ts --stringsdata "{{justfile_directory()}}/.dd-ui/Build/Intermediates.noindex/SupportFins.build"

# The tip cards' pictures (web/ui/tippics.js) as drawing data for the app (Features/Tips); `--check` is in `just gate`.
tippics-gen *args:
    deno run -A --config tools/tippics/deno.json tools/tippics/gen.ts {{args}}

# What's new (web/changelog.json) as the app's changelog.json and its version (app/Version.xcconfig); `--check` is in `just gate`.
whatsnew-gen *args:
    deno run -A --no-config tools/whatsnew/gen.js {{args}}

# The app's Swift compiler warnings against tools/gate/swift-warnings.baseline: a new one fails (S-09). Builds the app from
# clean in `.dd-warnings` (an incremental build only reports what it recompiles), under the machine lock. Part of `just gate`.
swift-warnings: app-core app-project
    tools/gate/swift_warnings.py
