import Metal
import simd

/// One spot whose first layer starts in midair, in the seated (plate) frame: the tip of its cone and the points
/// of its level edge (`web/ui/islands.js` `updateIslands`).
public struct IslandMarker: Equatable, Sendable {
    public var tip: SIMD3<Float>
    public var points: [SIMD3<Float>]

    public init(tip: SIMD3<Float>, points: [SIMD3<Float>]) {
        self.tip = tip
        self.points = points
    }
}

/// The red cones of Show midair starts (`web/ui/islands.js:42-77`): a cone under each start, tip on the start,
/// and a line along a level edge. Drawn over the part with no depth test, `RED` at opacity 0.9.
public enum IslandMarkers {
    /// `const s = Math.min(6, Math.max(1.5, 0.03 * Math.max(size.x, size.y, size.z)))`.
    public static func unit(extent: Float) -> Float { min(6, max(1.5, 0.03 * extent)) }

    /// The cones as interleaved position + normal floats (6 a vertex): radius `0.45 s`, height `s`, 16 sides, the
    /// tip at the start and the base disc `s` below it (`ConeGeometry` turned to +Z, centred `s / 2` under).
    public static func triangles(_ markers: [IslandMarker], extent: Float) -> [Float] {
        let s = unit(extent: extent), r = 0.45 * s
        var out: [Float] = []
        func vertex(_ p: SIMD3<Float>, _ n: SIMD3<Float>) { out += [p.x, p.y, p.z, n.x, n.y, n.z] }
        let sides = 16
        for m in markers {
            let tip = m.tip, base = SIMD3<Float>(tip.x, tip.y, tip.z - s)
            for i in 0..<sides {
                let a0 = Float(i) / Float(sides) * 2 * .pi, a1 = Float(i + 1) / Float(sides) * 2 * .pi
                let c0 = base + SIMD3(r * cos(a0), r * sin(a0), 0), c1 = base + SIMD3(r * cos(a1), r * sin(a1), 0)
                let n = simd_normalize(SIMD3<Float>(cos(a0) + cos(a1), sin(a0) + sin(a1), r / s))
                vertex(c0, n); vertex(c1, n); vertex(tip, n)
                vertex(base, SIMD3(0, 0, -1)); vertex(c1, SIMD3(0, 0, -1)); vertex(c0, SIMD3(0, 0, -1))
            }
        }
        return out
    }

    /// The line along a level edge: between its two farthest points (in plan), when they are further apart than
    /// the cone is high. Two vertices (packed xyz) per marker that has one.
    public static func lines(_ markers: [IslandMarker], extent: Float) -> [Float] {
        let s = unit(extent: extent)
        var out: [Float] = []
        for m in markers {
            guard var a = m.points.first else { continue }
            var b = a, far: Float = 0
            for p in m.points { for q in m.points {
                let d = hypot(p.x - q.x, p.y - q.y)
                if d > far { far = d; a = p; b = q }
            } }
            if far > s { out += [a.x, a.y, a.z, b.x, b.y, b.z] }
        }
        return out
    }
}

extension ViewerRenderer {
    /// Draw the markers (plate frame, so no model matrix), over everything.
    func encodeIslandMarkers(into enc: MTLRenderCommandEncoder) {
        guard islandCones != nil || islandLines != nil else { return }
        enc.setDepthStencilState(hudDepthState)
        var u = Uniforms(viewProj: camera.viewProjection, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1),
                         params: SIMD4(Self.noFace, 6, 0, 0))
        var c = SIMD4(Palette.islandRed, 0.9)
        if let cones = islandCones {
            enc.setRenderPipelineState(hudPipeline)
            enc.setFrontFacing(.counterClockwise)
            enc.setCullMode(.none)
            enc.setVertexBuffer(cones.buffer, offset: 0, index: 0)
            enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
            enc.setVertexBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
            enc.setFragmentBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
            enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: cones.vertices)
        }
        if let lines = islandLines {
            enc.setRenderPipelineState(linePipeline)
            enc.setVertexBuffer(lines.buffer, offset: 0, index: 0)
            enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
            enc.setFragmentBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
            enc.drawPrimitives(type: .line, vertexStart: 0, vertexCount: lines.vertices)
        }
    }

    /// Show a red cone at each marker; `extent` is the posed part's longest side. An empty list takes them off.
    public func setIslandMarkers(_ markers: [IslandMarker], extent: Float) {
        let tris = IslandMarkers.triangles(markers, extent: extent), lines = IslandMarkers.lines(markers, extent: extent)
        islandCones = tris.isEmpty ? nil : device.makeBuffer(bytes: tris, length: tris.count * 4, options: .storageModeShared)
            .map { ($0, tris.count / 6) }
        islandLines = lines.isEmpty ? nil : device.makeBuffer(bytes: lines, length: lines.count * 4, options: .storageModeShared)
            .map { ($0, lines.count / 3) }
        changeCount += 1
    }

    public var hasIslandMarkers: Bool { islandCones != nil }
}
