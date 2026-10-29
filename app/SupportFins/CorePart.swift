import Foundation
import Metal
import Observation
import SupportFinsCore
import Viewer
import simd

/// A `Part` of the engine as the viewer's `MeshSource`. The mesh and the flags are the Rust allocations
/// themselves (`GpuBuffer.metalBuffer`, no copy); the analysis runs on a background thread.
@MainActor
@Observable
final class CorePart: MeshSource {
    let part: Part
    /// The engine's description of the part. The import note changes once, when a saved session is restored onto it
    /// (`refreshInfo`), so it is observed.
    private(set) var info: PartInfo
    let vertexBuffer: MTLBuffer
    let vertexStride = 24
    let faceCount: Int
    let bounds: (min: SIMD3<Float>, max: SIMD3<Float>)
    private let device: MTLDevice

    init?(part: Part, device: MTLDevice) {
        self.part = part
        self.device = device
        let described = part.info()
        info = described
        guard let buffer = part.mesh().metalBuffer(device: device) else { return nil }
        vertexBuffer = buffer
        faceCount = Int(described.triangles)
        bounds = Self.bounds(of: buffer, vertices: faceCount * 3)
    }

    /// The part's description again: its import note says what a session restored (`restoreSession`, `afterLoad`).
    func refreshInfo() { info = part.info() }

    /// The unit normal of face `face` in the model frame: the engine's own (winding-derived, not the file's),
    /// which rides in the mesh buffer next to each vertex (`docs/FFI.md` `Part::mesh`), as `topology.nrm`
    /// (`web/ui/pose.js:315`).
    func faceNormal(_ face: Int) -> SIMD3<Double>? {
        guard face >= 0, face < faceCount else { return nil }
        let base = vertexBuffer.contents().bindMemory(to: Float.self, capacity: faceCount * 18)
        let o = face * 18 + 3
        return SIMD3(Double(base[o]), Double(base[o + 1]), Double(base[o + 2]))
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
        await analysis(thresholdDegrees: thresholdDegrees, rotation: rotation, wantsSummary: true)
    }

    /// `face_flags` and `analyze` each run the engine's analysis; a ring drag reads only the flags (the pivot and
    /// the box are the drag's own), so it skips the second one: half the time per shade.
    func analysis(thresholdDegrees: Double, rotation: simd_float3x3, wantsSummary: Bool) async -> MeshAnalysis {
        let part = self.part, device = self.device
        let rot = Self.columnMajor(rotation)
        return await Task.detached(priority: .userInitiated) {
            guard let flags = try? part.faceFlags(thresholdDeg: thresholdDegrees, rot: rot).metalBuffer(device: device) else {
                return MeshAnalysis(flags: device.makeBuffer(length: 4, options: .storageModeShared)!)
            }
            guard wantsSummary else { return MeshAnalysis(flags: flags) }
            guard let summary = try? part.analyze(thresholdDeg: thresholdDegrees, rot: rot) else {
                return MeshAnalysis(flags: device.makeBuffer(length: 4, options: .storageModeShared)!)
            }
            let o = summary.offset
            let z = summary.size
            return MeshAnalysis(flags: flags, offset: o.count == 3 ? SIMD3(Float(o[0]), Float(o[1]), Float(o[2])) : nil,
                                size: z.count == 3 ? SIMD3(Float(z[0]), Float(z[1]), Float(z[2])) : nil)
        }.value
    }

    /// The engine's `Rot3`: nine numbers, column-major, as a `simd_float3x3` stores them.
    static func columnMajor(_ m: simd_float3x3) -> [Double] {
        [m.columns.0, m.columns.1, m.columns.2].flatMap { [Double($0.x), Double($0.y), Double($0.z)] }
    }
}

/// `load_parts`: STL, OBJ, 3MF and STEP (one `Part` per object).
struct CoreLoader: PartLoading {
    struct NoFile: LocalizedError { var errorDescription: String? { "Open an STL, 3MF, STEP or OBJ file." } }
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
