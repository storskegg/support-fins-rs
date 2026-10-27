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
        .target(name: "SupportFinsCore", dependencies: ["sf_ffiFFI"], linkerSettings: [.linkedLibrary("c++")]),
        .testTarget(name: "SupportFinsCoreTests", dependencies: ["SupportFinsCore"]),
    ],
    swiftLanguageModes: [.v6]
)
