// swift-tools-version: 6.2
// The Swift face of crates/sf-ffi (docs/FFI.md). `just swift-bindings` generates
// Sources/SupportFinsCore/sf_ffi.swift and builds sf_ffiFFI.xcframework (not committed).
import PackageDescription

let package = Package(
    name: "SupportFinsCore",
    platforms: [.macOS(.v26)],
    products: [
        .library(name: "SupportFinsCore", targets: ["SupportFinsCore"]),
    ],
    targets: [
        .binaryTarget(name: "sf_ffiFFI", path: "sf_ffiFFI.xcframework"),
        // sf-cut is Manifold, C++: the static library needs the C++ runtime
        .target(name: "SupportFinsCore", dependencies: ["sf_ffiFFI", "SFCrashCapture"], linkerSettings: [.linkedLibrary("c++")]),
        // native crash capture (fix plan 1E): the fatal-signal and uncaught-exception handlers, in C (async-signal-safe)
        .target(name: "SFCrashCapture", linkerSettings: [.linkedFramework("Foundation")]),
        // a command-line child that crashes on purpose, for each signal and an uncaught NSException, with the handlers
        // installed: NativeCrashProbeTests runs it (a non-GUI process: no crash dialog) and reads its dump
        .target(name: "SFCrashProbeC"),
        .executableTarget(name: "sf-crash-probe", dependencies: ["SFCrashCapture", "SFCrashProbeC"]),
        .testTarget(name: "SupportFinsCoreTests", dependencies: ["SupportFinsCore", "sf-crash-probe"]),
    ],
    swiftLanguageModes: [.v6]
)
