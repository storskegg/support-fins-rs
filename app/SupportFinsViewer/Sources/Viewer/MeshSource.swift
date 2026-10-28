import Metal
import simd

/// The bits of `Part::face_flags` (`docs/FFI.md`): one byte per face of the analysis at a threshold and a
/// rotation. `kept` and `over` are `Analysis.kept` / `Analysis.over`, `onBed` is `Analysis.on_bed`.
public struct FaceFlags: OptionSet, Sendable {
    public let rawValue: UInt8
    public init(rawValue: UInt8) { self.rawValue = rawValue }
    /// Overhang by the angle test (`over`).
    public static let over = FaceFlags(rawValue: 1 << 0)
    /// An overhang big enough for a fin (`kept`): drawn red; `over` without it is a sliver, drawn amber.
    public static let kept = FaceFlags(rawValue: 1 << 1)
    /// Touches the bed (`on_bed`): drawn blue.
    public static let onBed = FaceFlags(rawValue: 1 << 2)
}

/// A part as the viewer needs it: an interleaved vertex buffer, three vertices a face (a triangle soup), and
/// the per-face flags for a pose. `SupportFinsCore.Part` conforms in the app (its `GpuBuffer`s are wrapped as
/// `MTLBuffer`s with no copy); the viewer is developed against `GeneratedMesh` meanwhile.
@MainActor
public protocol MeshSource: AnyObject {
    /// `f32` position (3) then normal (3) per vertex, in the model frame: 24 bytes when `vertexStride` is 24.
    var vertexBuffer: MTLBuffer { get }
    /// Bytes between two vertices (`GpuBuffer::stride`).
    var vertexStride: Int { get }
    /// Triangles; the buffer holds `3 * faceCount` vertices.
    var faceCount: Int { get }
    /// The model's axis-aligned bounds in the model frame (for framing and for seating a pose).
    var bounds: (min: SIMD3<Float>, max: SIMD3<Float>) { get }
    /// The analysis of a pose: one byte per face (`FaceFlags`) for this overhang threshold (degrees from the
    /// plate) and rotation (`rotation` takes model coordinates to print coordinates), and the engine's seating
    /// offset for it when it has one. Runs off the main actor in a real part (the engine is not instant).
    func analysis(thresholdDegrees: Double, rotation: simd_float3x3) async -> MeshAnalysis
}

/// What `MeshSource.analysis` returns. The buffer is shared memory the GPU reads; it is handed over once and
/// not touched again by the producer.
public struct MeshAnalysis: @unchecked Sendable {
    public var flags: MTLBuffer
    /// `Analysis.offset`: seats the rotated part on the plate. Nil: the viewer seats it from the bounds.
    public var offset: SIMD3<Float>?
    /// `Analysis.size`: the posed part's size, for the layer frames. Nil: the rotated bounds.
    public var size: SIMD3<Float>?

    public init(flags: MTLBuffer, offset: SIMD3<Float>? = nil, size: SIMD3<Float>? = nil) {
        self.flags = flags
        self.offset = offset
        self.size = size
    }
}

/// A dev stand-in that needs no engine: a cube, with the overhang test done on the CPU. Never used for a
/// real part (the engine's `face_flags` is the truth); it feeds the viewer and its render tests.
@MainActor
public final class GeneratedMesh: MeshSource {
    public let vertexBuffer: MTLBuffer
    public let vertexStride = 24
    public let faceCount: Int
    public let bounds: (min: SIMD3<Float>, max: SIMD3<Float>)
    private let device: MTLDevice
    private let normals: [SIMD3<Float>]
    private let centroids: [SIMD3<Float>]

    /// A cube of `edge` mm centred on `center` (the origin by default), 12 triangles wound counter-clockwise seen from outside.
    public static func cube(edge: Float, center: SIMD3<Float> = .zero, device: MTLDevice) -> GeneratedMesh {
        let h = edge / 2
        let c: [SIMD3<Float>] = ([
            [-h, -h, -h], [h, -h, -h], [h, h, -h], [-h, h, -h],
            [-h, -h, h], [h, -h, h], [h, h, h], [-h, h, h],
        ] as [SIMD3<Float>]).map { $0 + center }
        // (indices CCW from outside, outward normal)
        let quads: [([Int], SIMD3<Float>)] = [
            ([0, 3, 2, 1], [0, 0, -1]), ([4, 5, 6, 7], [0, 0, 1]),
            ([0, 1, 5, 4], [0, -1, 0]), ([2, 3, 7, 6], [0, 1, 0]),
            ([1, 2, 6, 5], [1, 0, 0]), ([3, 0, 4, 7], [-1, 0, 0]),
        ]
        var tris: [SIMD3<Float>] = []
        var norms: [SIMD3<Float>] = []
        for (q, n) in quads {
            for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                tris.append(contentsOf: t.map { c[$0] })
                norms.append(n)
            }
        }
        return GeneratedMesh(triangles: tris, faceNormals: norms, device: device)
    }

    /// Any soup of triangles (3 vertices each) with its face normals.
    public init(triangles: [SIMD3<Float>], faceNormals: [SIMD3<Float>], device: MTLDevice) {
        precondition(triangles.count == faceNormals.count * 3)
        self.device = device
        faceCount = faceNormals.count
        normals = faceNormals
        var data: [Float] = []
        data.reserveCapacity(triangles.count * 6)
        for (i, p) in triangles.enumerated() {
            let n = faceNormals[i / 3]
            data.append(contentsOf: [p.x, p.y, p.z, n.x, n.y, n.z])
        }
        vertexBuffer = device.makeBuffer(bytes: data, length: data.count * 4, options: .storageModeShared)!
        var lo = SIMD3<Float>(repeating: .infinity), hi = SIMD3<Float>(repeating: -.infinity)
        for p in triangles { lo = simd_min(lo, p); hi = simd_max(hi, p) }
        bounds = (lo, hi)
        centroids = stride(from: 0, to: triangles.count, by: 3).map { (triangles[$0] + triangles[$0 + 1] + triangles[$0 + 2]) / 3 }
    }

    public func analysis(thresholdDegrees: Double, rotation: simd_float3x3) async -> MeshAnalysis {
        let flags = DevOverhang.flags(normals: normals, centroids: centroids, thresholdDegrees: thresholdDegrees, rotation: rotation)
        return MeshAnalysis(flags: device.makeBuffer(bytes: flags, length: max(flags.count, 1), options: .storageModeShared)!)
    }
}

/// The overhang angle test, for `GeneratedMesh` only: a face whose underside leans less than the threshold
/// from the plate needs support; one lying on z = 0 touches the bed.
public enum DevOverhang {
    public static func flags(normals: [SIMD3<Float>], centroids: [SIMD3<Float>], thresholdDegrees: Double,
                             rotation: simd_float3x3) -> [UInt8] {
        // seat: centre in plan, lowest point on the plate
        var minZ = Float.infinity
        for c in centroids { minZ = min(minZ, (rotation * c).z) }
        let limit = Float(cos(thresholdDegrees * .pi / 180))
        return zip(normals, centroids).map { n, c in
            let nz = (rotation * n).z
            let z = (rotation * c).z - minZ
            var f: FaceFlags = []
            if -nz > limit {
                if z < 1e-3 && -nz > 0.999 { f.insert(.onBed) } else { f.formUnion([.over, .kept]) }
            }
            return f.rawValue
        }
    }
}

/// Euler XYZ as three.js builds it (`Rx * Ry * Rz`), in degrees: the pose the temporary sliders set.
public func rotationMatrix(degrees d: SIMD3<Float>) -> simd_float3x3 {
    let r = d * .pi / 180
    let (cx, sx, cy, sy, cz, sz) = (cos(r.x), sin(r.x), cos(r.y), sin(r.y), cos(r.z), sin(r.z))
    let rx = simd_float3x3(columns: (SIMD3(1, 0, 0), SIMD3(0, cx, sx), SIMD3(0, -sx, cx)))
    let ry = simd_float3x3(columns: (SIMD3(cy, 0, -sy), SIMD3(0, 1, 0), SIMD3(sy, 0, cy)))
    let rz = simd_float3x3(columns: (SIMD3(cz, sz, 0), SIMD3(-sz, cz, 0), SIMD3(0, 0, 1)))
    return rx * ry * rz
}

/// The offset that seats a rotated box on the plate (centred in plan, lowest corner on z = 0): what the
/// engine's `Analysis.offset` is for a real part. Corners of the model's box are exact for a box and a
/// conservative stand-in otherwise.
public func seatingOffset(bounds: (min: SIMD3<Float>, max: SIMD3<Float>), rotation: simd_float3x3) -> SIMD3<Float> {
    var lo = SIMD3<Float>(repeating: .infinity), hi = SIMD3<Float>(repeating: -.infinity)
    for i in 0..<8 {
        let c = SIMD3<Float>(i & 1 == 0 ? bounds.min.x : bounds.max.x,
                             i & 2 == 0 ? bounds.min.y : bounds.max.y,
                             i & 4 == 0 ? bounds.min.z : bounds.max.z)
        let p = rotation * c
        lo = simd_min(lo, p)
        hi = simd_max(hi, p)
    }
    return SIMD3(-(lo.x + hi.x) / 2, -(lo.y + hi.y) / 2, -lo.z)
}
