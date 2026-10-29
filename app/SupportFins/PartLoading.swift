import Foundation
import Metal
import Viewer

/// Where a window's mesh comes from: `CoreLoader` (the engine's `load_parts`) in the app, `GeneratedLoader`
/// (a cube) for tests and previews.
@MainActor
protocol PartLoading {
    func load(url: URL?, device: MTLDevice) async throws -> [LoadedPart]
}

struct LoadedPart {
    var name: String
    var mesh: any MeshSource
}

struct GeneratedLoader: PartLoading {
    func load(url: URL?, device: MTLDevice) async throws -> [LoadedPart] {
        [LoadedPart(name: url?.lastPathComponent ?? JS.t("Cube"), mesh: GeneratedMesh.cube(edge: 40, device: device))]
    }
}
