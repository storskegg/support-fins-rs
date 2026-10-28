// swift-tools-version: 6.2
import PackageDescription

// The viewer: the Z-up camera, the Metal renderer and its frame loop, the plate, and the picking
// pipeline. It knows nothing about the engine: a part reaches it through `MeshSource`, which
// `SupportFinsCore`'s `Part` conforms to in the app target.
let package = Package(
    name: "SupportFinsViewer",
    platforms: [.macOS(.v26)],
    products: [
        .library(name: "Viewer", targets: ["Viewer"]),
        .executable(name: "viewer-bench", targets: ["ViewerBench"]),
    ],
    targets: [
        .target(name: "Viewer", swiftSettings: [.swiftLanguageMode(.v6)]),
        .executableTarget(name: "ViewerBench", dependencies: ["Viewer"], swiftSettings: [.swiftLanguageMode(.v6)]),
        .testTarget(name: "ViewerTests", dependencies: ["Viewer"], swiftSettings: [.swiftLanguageMode(.v6)]),
    ]
)
