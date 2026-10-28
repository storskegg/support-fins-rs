import Dispatch
import Foundation
import Metal
import simd

/// A generated heavy scene for the frame-time benchmark: a lumpy sphere of 1-2 million triangles (every face
/// direction, so the overhang colours are mixed) and a few thousand fins and tines standing under it, as the
/// engine's fin buffer holds them. No engine involved.
@MainActor
public final class StressMesh: MeshSource {
    public let vertexBuffer: MTLBuffer
    public let vertexStride = 24
    public let faceCount: Int
    public let bounds: (min: SIMD3<Float>, max: SIMD3<Float>)
    private let device: MTLDevice

    /// About `triangles` triangles (rounded to a whole lat/long grid).
    public init(triangles: Int, device: MTLDevice) {
        self.device = device
        let m = max(8, Int((Double(triangles) / 4).squareRoot()))      // latitude bands; longitude columns = 2 m
        let n = 2 * m
        faceCount = 2 * n * m
        let radius: Float = 45
        func point(_ i: Int, _ j: Int) -> SIMD3<Float> {
            let u = Float(i % n) / Float(n) * 2 * .pi, v = Float(j) / Float(m) * .pi
            let r = radius * (1 + 0.10 * sin(7 * u) * sin(9 * v) + 0.04 * sin(23 * u + 3 * v) * cos(17 * v))
            return SIMD3(r * sin(v) * cos(u), r * sin(v) * sin(u), r * cos(v))
        }
        var grid = [SIMD3<Float>](repeating: .zero, count: (n + 1) * (m + 1))
        for j in 0...m { for i in 0...n { grid[j * (n + 1) + i] = point(i, j) } }
        let length = faceCount * 3 * 24
        let buffer = device.makeBuffer(length: length, options: .storageModeShared)!
        let out = Slab(base: buffer.contents().bindMemory(to: Float.self, capacity: faceCount * 18))
        grid.withUnsafeBufferPointer { gp in
            let g = GridRef(base: gp.baseAddress!)
            DispatchQueue.concurrentPerform(iterations: m) { j in
                for i in 0..<n {
                    let p00 = g.base[j * (n + 1) + i], p10 = g.base[j * (n + 1) + i + 1]
                    let p01 = g.base[(j + 1) * (n + 1) + i], p11 = g.base[(j + 1) * (n + 1) + i + 1]
                    var f = (j * n + i) * 2
                    for tri in [(p00, p10, p11), (p00, p11, p01)] {
                        var nrm = simd_cross(tri.1 - tri.0, tri.2 - tri.0)
                        var (a, b, c) = tri
                        let centre = (a + b + c) / 3
                        // wound counter-clockwise seen from outside
                        if simd_dot(nrm, centre) < 0 { swap(&b, &c); nrm = -nrm }
                        let l = simd_length(nrm)
                        nrm = l > 0 ? nrm / l : SIMD3(0, 0, 1)
                        let o = f * 18
                        for (k, p) in [a, b, c].enumerated() {
                            out.base[o + k * 6] = p.x; out.base[o + k * 6 + 1] = p.y; out.base[o + k * 6 + 2] = p.z
                            out.base[o + k * 6 + 3] = nrm.x; out.base[o + k * 6 + 4] = nrm.y; out.base[o + k * 6 + 5] = nrm.z
                        }
                        f += 1
                    }
                }
            }
        }
        vertexBuffer = buffer
        bounds = (SIMD3(repeating: -radius * 1.15), SIMD3(repeating: radius * 1.15))
    }

    public func analysis(thresholdDegrees: Double, rotation: simd_float3x3) async -> MeshAnalysis {
        MeshAnalysis(flags: StressScene.flags(of: self, thresholdDegrees: thresholdDegrees, rotation: rotation, device: device))
    }
}

private struct Slab: @unchecked Sendable { let base: UnsafeMutablePointer<Float> }
private struct GridRef: @unchecked Sendable { let base: UnsafePointer<SIMD3<Float>> }

public enum StressScene {
    /// The overhang flags of a pose, the way the engine's `face_flags` would colour them (angle test only),
    /// computed in parallel into a fresh shared buffer.
    @MainActor
    public static func flags(of mesh: StressMesh, thresholdDegrees: Double, rotation: simd_float3x3, device: MTLDevice) -> MTLBuffer {
        let n = mesh.faceCount
        let buffer = device.makeBuffer(length: max(n, 1), options: .storageModeShared)!
        let verts = Slab(base: mesh.vertexBuffer.contents().bindMemory(to: Float.self, capacity: n * 18))
        let dst = buffer.contents().bindMemory(to: UInt8.self, capacity: n)
        let dstRef = Bytes(base: dst)
        let limit = Float(cos(thresholdDegrees * .pi / 180))
        let rz = SIMD3<Float>(rotation.columns.0.z, rotation.columns.1.z, rotation.columns.2.z)
        let chunks = 64
        DispatchQueue.concurrentPerform(iterations: chunks) { c in
            for f in (n * c / chunks)..<(n * (c + 1) / chunks) {
                let o = f * 18 + 3
                let nz = rz.x * verts.base[o] + rz.y * verts.base[o + 1] + rz.z * verts.base[o + 2]
                var bits: UInt8 = 0
                if -nz > limit { bits = FaceFlags([.over, .kept]).rawValue }
                dstRef.base[f] = bits
            }
        }
        return buffer
    }

    /// Fins (thin walls) and tines (small eight-sided cones) standing on the plate under the part, as the
    /// fin buffer's interleaved triangle soup (print space). `hiddenFins` ranges of the buffer are marked
    /// removed, the way a user's removals leave it.
    @MainActor
    public static func supports(fins: Int, tines: Int, device: MTLDevice, seed: UInt64 = 7) -> SupportLayer {
        var rng = SplitMix(seed)
        var data: [Float] = []
        data.reserveCapacity((fins * 36 + tines * 48) * 6)
        func tri(_ a: SIMD3<Float>, _ b: SIMD3<Float>, _ c: SIMD3<Float>) {
            var nrm = simd_cross(b - a, c - a)
            let l = simd_length(nrm)
            nrm = l > 0 ? nrm / l : SIMD3(0, 0, 1)
            for p in [a, b, c] { data += [p.x, p.y, p.z, nrm.x, nrm.y, nrm.z] }
        }
        func spot() -> SIMD3<Float> {
            while true {
                let x = Float.random(in: -40...40, using: &rng), y = Float.random(in: -40...40, using: &rng)
                if x * x + y * y < 1600 { return SIMD3(x, y, 0) }
            }
        }
        for _ in 0..<fins {
            let c = spot(), h = Float.random(in: 3...35, using: &rng), a = Float.random(in: 0..<(2 * .pi), using: &rng)
            let along = SIMD3<Float>(cos(a), sin(a), 0) * 4, across = SIMD3<Float>(-sin(a), cos(a), 0) * 0.2
            let up = SIMD3<Float>(0, 0, h)
            // a box: 6 quads, 12 triangles
            let o = [c - along - across, c + along - across, c + along + across, c - along + across]
            let t = o.map { $0 + up }
            for i in 0..<4 {
                let j = (i + 1) % 4
                tri(o[i], o[j], t[j]); tri(o[i], t[j], t[i])
            }
            tri(t[0], t[1], t[2]); tri(t[0], t[2], t[3])
            tri(o[0], o[2], o[1]); tri(o[0], o[3], o[2])
        }
        for _ in 0..<tines {
            let c = spot(), h = Float.random(in: 2...12, using: &rng)
            let apex = c + SIMD3(0, 0, h)
            let ring = (0..<8).map { k -> SIMD3<Float> in
                let a = Float(k) / 8 * 2 * .pi
                return c + SIMD3(cos(a), sin(a), 0) * 0.8
            }
            for k in 0..<8 { tri(ring[k], ring[(k + 1) % 8], apex) }
            for k in 1..<7 { tri(ring[0], ring[k + 1], ring[k]) }
        }
        let finVertices = data.count / 6
        // the bed pad: a disk of 64 triangles
        var pad: [Float] = []
        for k in 0..<64 {
            let a0 = Float(k) / 64 * 2 * .pi, a1 = Float(k + 1) / 64 * 2 * .pi
            for p in [SIMD3<Float>(0, 0, 0), SIMD3(cos(a0), sin(a0), 0) * 44, SIMD3(cos(a1), sin(a1), 0) * 44] {
                pad += [p.x, p.y, 0.5, 0, 0, 1]
            }
        }
        let finBuffer = device.makeBuffer(bytes: data, length: max(data.count * 4, 4), options: .storageModeShared)!
        let padBuffer = device.makeBuffer(bytes: pad, length: pad.count * 4, options: .storageModeShared)!
        return SupportLayer(fins: finBuffer, finVertices: finVertices, pad: padBuffer, padVertices: pad.count / 6)
    }
}

private struct Bytes: @unchecked Sendable { let base: UnsafeMutablePointer<UInt8> }

struct SplitMix: RandomNumberGenerator {
    var state: UInt64
    init(_ seed: UInt64) { state = seed }
    mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
        return z ^ (z >> 31)
    }
}
