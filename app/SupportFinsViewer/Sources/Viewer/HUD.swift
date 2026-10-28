import Metal
import simd

/// The Strength view's two drawings (`web/ui/strength.js`): the load arrow and the layer frames. Both are
/// HUD-style, drawn over the part and the supports with no depth test, as the web draws them
/// (`depthTest = false`, `renderOrder` 10 for the frames and 12 for the arrow).

/// The arrow of a load: a shaft and a cone through the middle of the part, along the load direction.
public enum LoadArrowGeometry {
    /// `len = Math.max(10, maxDim * 0.6)` (`updateLoadArrowMesh`).
    public static func length(extent: Float) -> Float { max(10, extent * 0.6) }

    /// The arrow's triangles as interleaved position + normal floats (6 a vertex), in the frame `center` and
    /// `direction` are given in. The tail is `-direction * len / 2` from `center`, so the whole arrow spans the
    /// part; the head is `min(len * 0.28, 12)` long and `min(len * 0.18, 7)` wide (three.js `ArrowHelper`'s
    /// `setLength(len, headLength, headWidth)`, whose cone has radius `headWidth / 2`).
    public static func triangles(center: SIMD3<Float>, direction: SIMD3<Float>, extent: Float) -> [Float] {
        let len = length(extent: extent)
        let headLength = min(len * 0.28, 12), headRadius = min(len * 0.18, 7) / 2
        let d = simd_normalize(direction)
        let helper = abs(d.z) < 0.9 ? SIMD3<Float>(0, 0, 1) : SIMD3<Float>(1, 0, 0)
        let u = simd_normalize(simd_cross(d, helper)), v = simd_cross(d, u)
        let tail = center - d * (len / 2)
        let neck = tail + d * (len - headLength), tip = tail + d * len
        let shaftRadius = max(0.25, headRadius * 0.12)
        var out: [Float] = []
        func vertex(_ p: SIMD3<Float>, _ n: SIMD3<Float>) { out += [p.x, p.y, p.z, n.x, n.y, n.z] }
        func ring(_ a: Float) -> SIMD3<Float> { cos(a) * u + sin(a) * v }
        let sides = 16
        for i in 0..<sides {
            let a0 = Float(i) / Float(sides) * 2 * .pi, a1 = Float(i + 1) / Float(sides) * 2 * .pi
            let r0 = ring(a0), r1 = ring(a1)
            // the shaft: a prism from the tail to the neck
            let (s0, s1) = (r0 * shaftRadius, r1 * shaftRadius)
            for (p, q, r) in [(tail + s0, tail + s1, neck + s1), (tail + s0, neck + s1, neck + s0)] {
                vertex(p, simd_normalize(p - tail)); vertex(q, simd_normalize(q - tail)); vertex(r, simd_normalize(r - neck))
            }
            // the cone: the base ring to the tip
            let (c0, c1) = (neck + r0 * headRadius, neck + r1 * headRadius)
            let n = simd_normalize(r0 + r1 + d * (headRadius / headLength))
            vertex(c0, n); vertex(c1, n); vertex(tip, n)
            // its base disc, facing back along the shaft
            vertex(neck, -d); vertex(c1, -d); vertex(c0, -d)
        }
        return out
    }
}

/// The layer frames: a stack of horizontal rectangles around the part, world-aligned, that show how the print
/// is layered (`rebuildLayerViz`, `web/ui/strength.js`).
public enum LayerFrames {
    public static let maxFrames = 16

    /// `Math.max(3, Math.min(16, Math.round(size.z / 8) + 1))`: a frame about every 8 mm.
    public static func count(sizeZ: Float) -> Int { max(3, min(maxFrames, Int((sizeZ / 8).rounded()) + 1)) }

    /// Line vertices (two per segment, packed xyz) for a posed part `size` big, centred on the plan origin and
    /// resting on `z = 0`; the frames reach a little past the silhouette (`max(4, 0.12 * max(x, y))`).
    public static func lines(size: SIMD3<Float>) -> [Float] {
        let n = count(sizeZ: size.z)
        let m = max(4, 0.12 * max(size.x, size.y))
        let hx = size.x / 2 + m, hy = size.y / 2 + m
        var out: [Float] = []
        for k in 0..<n {
            let z = size.z * Float(k) / Float(n - 1)
            let corners: [SIMD2<Float>] = [[-hx, -hy], [hx, -hy], [hx, hy], [-hx, hy]]
            for i in 0..<4 {
                let a = corners[i], b = corners[(i + 1) % 4]
                out += [a.x, a.y, z, b.x, b.y, z]
            }
        }
        return out
    }
}

extension ViewerRenderer {
    /// Draw the layer frames (print space) and then the load arrow (model space, so it turns with the part).
    func encodeHUD(into enc: MTLRenderCommandEncoder) {
        guard layerFrames != nil || loadArrow != nil else { return }
        enc.setDepthStencilState(hudDepthState)
        var u = Uniforms(viewProj: camera.viewProjection, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1),
                         params: SIMD4(Self.noFace, 6, 0, 0))
        if let frames = layerFrames {
            enc.setRenderPipelineState(linePipeline)
            var c = SIMD4(Palette.layers, 0.4)
            enc.setVertexBuffer(frames.buffer, offset: 0, index: 0)
            enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
            enc.setFragmentBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
            enc.drawPrimitives(type: .line, vertexStart: 0, vertexCount: frames.vertices)
        }
        if let arrow = loadArrow {
            u.model = modelMatrix
            enc.setRenderPipelineState(hudPipeline)
            enc.setFrontFacing(.counterClockwise)
            enc.setCullMode(.none)
            var c = SIMD4(Palette.loadArrow, 1)
            enc.setVertexBuffer(arrow.buffer, offset: 0, index: 0)
            enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
            enc.setVertexBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
            enc.setFragmentBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
            enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: arrow.vertices)
        }
    }

    /// Show the load arrow through `center` (model space) along `direction` (the part's own frame, so it turns
    /// with the part); `extent` is the part's longest side. Nil takes it off.
    public func setLoadArrow(center: SIMD3<Float>, direction: SIMD3<Float>, extent: Float) {
        let floats = LoadArrowGeometry.triangles(center: center, direction: direction, extent: extent)
        loadArrow = device.makeBuffer(bytes: floats, length: floats.count * 4, options: .storageModeShared)
            .map { ($0, floats.count / 6) }
        changeCount += 1
    }

    public func clearLoadArrow() {
        guard loadArrow != nil else { return }
        loadArrow = nil
        changeCount += 1
    }

    /// Show the layer frames around a posed part of this `size`. Nil takes them off.
    public func setLayerFrames(size: SIMD3<Float>) {
        let floats = LayerFrames.lines(size: size)
        layerFrames = device.makeBuffer(bytes: floats, length: floats.count * 4, options: .storageModeShared)
            .map { ($0, floats.count / 3) }
        changeCount += 1
    }

    public func clearLayerFrames() {
        guard layerFrames != nil else { return }
        layerFrames = nil
        changeCount += 1
    }

    public var hasLoadArrow: Bool { loadArrow != nil }
    public var hasLayerFrames: Bool { layerFrames != nil }
}
