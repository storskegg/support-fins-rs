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
    let vertexStride = 24
    let faceCount: Int
    private let device: MTLDevice
    /// The Metal mesh and its box, once made (C-05). A file's objects are described from their soups alone (the
    /// picker needs a name, a triangle count and a size); only an object that is shown is welded and meshed.
    @ObservationIgnored private var loaded: (buffer: MTLBuffer, bounds: (min: SIMD3<Float>, max: SIMD3<Float>))?

    /// The part with its mesh made now (it welds the part, on the calling thread). `nil` when the GPU cannot share it.
    convenience init?(part: Part, device: MTLDevice) {
        self.init(lazy: part, device: device)
        guard adopt(part.mesh()) else { return nil }
    }

    /// The part described but not welded or meshed: `prepare()` makes the mesh before the part is shown.
    init(lazy part: Part, device: MTLDevice) {
        self.part = part
        self.device = device
        let described = part.info()
        info = described
        faceCount = Int(described.triangles)
    }

    var vertexBuffer: MTLBuffer { mesh().buffer }
    var bounds: (min: SIMD3<Float>, max: SIMD3<Float>) { mesh().bounds }

    /// Weld and mesh the part off the main thread (the engine welds on first use). `false` when the GPU cannot share it.
    func prepare() async -> Bool {
        if loaded != nil { return true }
        let part = self.part
        let gpu = await Task.detached(priority: .userInitiated) { part.mesh() }.value
        return adopt(gpu)
    }

    private func adopt(_ gpu: GpuBuffer) -> Bool {
        if loaded != nil { return true }
        guard let buffer = gpu.metalBuffer(device: device) else { return false }
        loaded = (buffer, Self.bounds(of: buffer, vertices: faceCount * 3))
        refreshInfo()       // the weld's time is known now
        return true
    }

    /// The mesh, made here if `prepare()` has not (a caller that needs it before it was prepared pays on this thread).
    private func mesh() -> (buffer: MTLBuffer, bounds: (min: SIMD3<Float>, max: SIMD3<Float>)) {
        if loaded == nil { _ = adopt(part.mesh()) }
        // a mesh the GPU cannot share: an empty buffer, so nothing draws and nothing reads past its end
        return loaded ?? (device.makeBuffer(length: max(faceCount * 72, 24), options: .storageModeShared)!, (.zero, .zero))
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

/// `load_parts`: STL, OBJ, 3MF and STEP (one `Part` per object). STEP is read in the StepReader XPC service (X-01).
struct CoreLoader: PartLoading {
    struct NoFile: LocalizedError { var errorDescription: String? { "Open an STL, 3MF, STEP or OBJ file." } }
    struct Unreadable: LocalizedError { var errorDescription: String? { JS.t("The file's mesh could not be shared with the GPU.") } }

    func load(url: URL?, device: MTLDevice, allowLarge: Bool) async throws -> [LoadedPart] {
        guard let url else { throw NoFile() }
        let parts = try await Self.parts(at: url.path(percentEncoded: false), allowLarge: allowLarge)
        // C-05: the objects are described, not welded or meshed; a file with one object is shown at once, so its mesh
        // is made now (off the main thread), and a file with several waits for the picker's choice (`ViewerModel`)
        let cores = parts.map { CorePart(lazy: $0, device: device) }
        if cores.count == 1, await cores[0].prepare() == false { throw Unreadable() }
        return cores.map { LoadedPart(name: $0.info.name, mesh: $0) }
    }

    /// The engine's parts of a file: `load_parts`, except that a file it would read as STEP (`reads_as_step`) is read
    /// by OpenCascade in the StepReader XPC service and made into parts by `load_step_parts_with`, so the app's process never
    /// runs OpenCascade.
    nonisolated static func parts(at path: String, allowLarge: Bool = false, stepTimeout: Duration = StepReader.timeout) async throws -> [Part] {
        let options = LoadOptions(allowLarge: allowLarge)
        let step = try await Task.detached(priority: .userInitiated) { try readsAsStep(path: path) }.value
        guard step else { return try await Task.detached(priority: .userInitiated) { try loadPartsWith(path: path, options: options) }.value }
        let file = try Data(contentsOf: URL(fileURLWithPath: path), options: .mappedIfSafe)
        let kernel = try await StepReader.read(file, timeout: stepTimeout)
        return try await Task.detached(priority: .userInitiated) { try loadStepPartsWith(path: path, kernel: kernel, options: options) }.value
    }
}
