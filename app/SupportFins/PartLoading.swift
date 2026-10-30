import Foundation
import Metal
import Viewer

/// Where a window's mesh comes from: `CoreLoader` (the engine's `load_parts`) in the app, `GeneratedLoader`
/// (a cube) for tests and previews.
@MainActor
protocol PartLoading {
    /// `allowLarge`: load past the "Load anyway?" limits (`load_parts_with`); a file over one throws `FfiError.LimitExceeded`.
    func load(url: URL?, device: MTLDevice, allowLarge: Bool) async throws -> [LoadedPart]
}

struct LoadedPart {
    var name: String
    var mesh: any MeshSource
}

struct GeneratedLoader: PartLoading {
    func load(url: URL?, device: MTLDevice, allowLarge: Bool) async throws -> [LoadedPart] {
        [LoadedPart(name: url?.lastPathComponent ?? JS.t("Cube"), mesh: GeneratedMesh.cube(edge: 40, device: device))]
    }
}
