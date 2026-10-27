import Foundation
import Metal
import SupportFinsCore
import Viewer
import simd

/// A `Part` of the engine as the viewer's `MeshSource`. The mesh and the flags are the Rust allocations
/// themselves (`GpuBuffer.metalBuffer`, no copy); the analysis runs on a background thread.
@MainActor
final class CorePart: MeshSource {
    let part: Part
    let info: PartInfo
    let vertexBuffer: MTLBuffer
    let vertexStride = 24
    let faceCount: Int
    let bounds: (min: SIMD3<Float>, max: SIMD3<Float>)
    private let device: MTLDevice

    init?(part: Part, device: MTLDevice) {
        self.part = part
        self.device = device
        info = part.info()
        guard let buffer = part.mesh().metalBuffer(device: device) else { return nil }
        vertexBuffer = buffer
        faceCount = Int(info.triangles)
        bounds = Self.bounds(of: buffer, vertices: faceCount * 3)
    }

    /// The model-frame box of the interleaved vertices (position first, six floats each). A file's objects sit
    /// where the file puts them, so the box is the mesh's own, not centred.
    static func bounds(of buffer: MTLBuffer, vertices n: Int) -> (min: SIMD3<Float>, max: SIMD3<Float>) {
        guard n > 0 else { return (.zero, .zero) }
        let base = buffer.contents().bindMemory(to: Float.self, capacity: n * 6)
        var lo = SIMD3<Float>(repeating: .infinity), hi = SIMD3<Float>(repeating: -.infinity)
        for i in 0..<n {
            let p = SIMD3<Float>(base[i * 6], base[i * 6 + 1], base[i * 6 + 2])
            lo = simd_min(lo, p)
            hi = simd_max(hi, p)
        }
        return (lo, hi)
    }

    func analysis(thresholdDegrees: Double, rotation: simd_float3x3) async -> MeshAnalysis {
        let part = self.part, device = self.device
        let rot = Self.columnMajor(rotation)
        return await Task.detached(priority: .userInitiated) {
            guard let flags = try? part.faceFlags(thresholdDeg: thresholdDegrees, rot: rot).metalBuffer(device: device),
                  let summary = try? part.analyze(thresholdDeg: thresholdDegrees, rot: rot) else {
                return MeshAnalysis(flags: device.makeBuffer(length: 4, options: .storageModeShared)!)
            }
            let o = summary.offset
            return MeshAnalysis(flags: flags, offset: o.count == 3 ? SIMD3(Float(o[0]), Float(o[1]), Float(o[2])) : nil)
        }.value
    }

    /// The engine's `Rot3`: nine numbers, column-major, as a `simd_float3x3` stores them.
    static func columnMajor(_ m: simd_float3x3) -> [Double] {
        [m.columns.0, m.columns.1, m.columns.2].flatMap { [Double($0.x), Double($0.y), Double($0.z)] }
    }
}

/// `load_parts`: STL, OBJ and 3MF (one `Part` per object).
struct CoreLoader: PartLoading {
    struct NoFile: LocalizedError { var errorDescription: String? { "Open an STL, OBJ or 3MF file." } }
    struct Unreadable: LocalizedError { var errorDescription: String? { "The file's mesh could not be shared with the GPU." } }

    func load(url: URL?, device: MTLDevice) async throws -> [LoadedPart] {
        guard let url else { throw NoFile() }
        let path = url.path(percentEncoded: false)
        let parts = try await Task.detached(priority: .userInitiated) { try loadParts(path: path) }.value
        return try parts.map { part in
            guard let core = CorePart(part: part, device: device) else { throw Unreadable() }
            return LoadedPart(name: core.info.name, mesh: core)
        }
    }
}
