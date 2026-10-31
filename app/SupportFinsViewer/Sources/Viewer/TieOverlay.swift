import Metal
import simd

/// One neighbour link of the plate being edited (issue #50): the line between two models' outlines that a tie pair
/// crosses, in the plate frame (the frame of a model's spot on its plate, the bed's centre at the origin).
public struct TieLinkLine: Equatable, Sendable {
    public var from: SIMD3<Float>
    public var to: SIMD3<Float>
    /// The pair is tied; off, it is a pair the detection found and the user switched off (drawn grey, a click turns it on).
    public var enabled: Bool
    /// Added by the user (two models clicked), not found by the detection.
    public var manual: Bool

    public init(from: SIMD3<Float>, to: SIMD3<Float>, enabled: Bool, manual: Bool) {
        (self.from, self.to, self.enabled, self.manual) = (from, to, enabled, manual)
    }
}

/// What plate ties put over the plate being edited: the ties, drawn like the fins, and the links, dashed over everything.
/// Both are in the plate frame; the renderer draws them about the edited bed (`PlateScene.beds[edited]`), so a swap of
/// the live model (which shifts the world) needs no new scene.
public struct TieScene: @unchecked Sendable {
    public var ties: OverlayMesh?
    public var links: [TieLinkLine] = []
    /// The ties lag the plate (a rebuild is out): drawn faded.
    public var stale = false

    public init(ties: OverlayMesh? = nil, links: [TieLinkLine] = [], stale: Bool = false) {
        (self.ties, self.links, self.stale) = (ties, links, stale)
    }
}

/// The colours of plate ties.
public enum TiePalette {
    /// The ties: the fin colour, they print with the fins.
    public static let tie = Palette.fin
    public static let link = Palette.hex(0x4da3ff)
    public static let manual = Palette.hex(0xffb347)
    public static let off = Palette.hex(0x8a8f98)
    /// Link width in points, and its dashes in mm.
    public static let width: Float = 2.5
    public static let dash: Float = 1.2
    public static let space: Float = 0.8
}

/// The renderer's state for plate ties (one object, so the renderer class gains one property).
@MainActor
final class TieRenderState {
    var scene: TieScene?
}

extension ViewerRenderer {
    /// Show the plate's ties and links (nil takes them off).
    public func setTieScene(_ scene: TieScene?) {
        if scene == nil && tieState.scene == nil { return }
        tieState.scene = scene
        changeCount += 1
    }

    public var tieScene: TieScene? { tieState.scene }

    /// The edited bed's origin in the scene: the plate frame drawn there.
    var editedBed: SIMD2<Float> {
        let beds = plateScene.beds
        guard !beds.isEmpty else { return .zero }
        return beds[min(max(plateScene.edited, 0), beds.count - 1)]
    }

    /// The dashes of a link from `a` to `b`: (start, end) pairs, `dash` mm on and `space` mm off, the last one cut short.
    public static func dashes(from a: SIMD3<Float>, to b: SIMD3<Float>, dash: Float = TiePalette.dash, space: Float = TiePalette.space) -> [(SIMD3<Float>, SIMD3<Float>)] {
        let length = simd_distance(a, b)
        guard length > 1e-4, dash > 0 else { return [] }
        let d = (b - a) / length
        var out: [(SIMD3<Float>, SIMD3<Float>)] = []
        var t: Float = 0
        while t < length, out.count < 10_000 {
            out.append((a + d * t, a + d * min(t + dash, length)))
            t += dash + space
        }
        return out
    }

    /// The ties (depth-tested, lit like the fins) and the links over everything.
    func encodeTieScene(into enc: MTLRenderCommandEncoder) {
        guard let s = tieState.scene else { return }
        let moved = camera.viewProjection * simd_float4x4(translation: SIMD3(editedBed, 0))
        var u = Uniforms(viewProj: moved, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1), params: SIMD4(Self.noFace, 6, 0, 0))
        if let t = s.ties, t.vertices > 0 {
            enc.setFrontFacing(.counterClockwise)
            enc.setCullMode(.none)
            enc.setDepthStencilState(depthState)
            enc.setRenderPipelineState(s.stale ? staleSupportPipeline : supportPipeline)
            var opacity = Self.staleOpacity
            if s.stale { enc.setFragmentBytes(&opacity, length: MemoryLayout<Float>.size, index: 0) }
            var c = SIMD4(TiePalette.tie, 1)
            enc.setVertexBuffer(t.buffer, offset: 0, index: 0)
            enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
            enc.setVertexBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
            enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: t.vertices)
        }
        guard !s.links.isEmpty else { return }
        enc.setRenderPipelineState(ringPipeline)
        enc.setDepthStencilState(hudDepthState)
        enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
        var params = SIMD4<Float>(viewport.x, viewport.y, TiePalette.width * pixelScale, 0)
        enc.setVertexBytes(&params, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
        for link in s.links {
            var c = SIMD4(link.enabled ? (link.manual ? TiePalette.manual : TiePalette.link) : TiePalette.off, link.enabled ? 1 : 0.7)
            enc.setFragmentBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
            for (p, q) in Self.dashes(from: link.from, to: link.to) {
                let flat: [Float] = [p.x, p.y, p.z, q.x, q.y, q.z]
                enc.setVertexBytes(flat, length: flat.count * 4, index: 0)
                enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 6)
            }
        }
    }

    /// The link under pixel (`x`, `y`) of a `width x height` view (origin top left), within `slop` points of it: its
    /// index in `tieScene.links`, the nearest; nil for none.
    public func linkAt(x: Int, y: Int, width: Int, height: Int, slop: Float = 6) -> Int? {
        guard let links = tieState.scene?.links, !links.isEmpty, width > 0, height > 0 else { return nil }
        let shift = SIMD3(editedBed, 0)
        func screen(_ p: SIMD3<Float>) -> SIMD2<Float>? {
            let c = camera.viewProjection * SIMD4(p + shift, 1)
            guard c.w > 1e-6 else { return nil }
            return SIMD2((c.x / c.w + 1) / 2 * Float(width), (1 - c.y / c.w) / 2 * Float(height))
        }
        let at = SIMD2(Float(x), Float(y)), limit = slop * max(pixelScale, 1)
        var best: (k: Int, d: Float)?
        for (k, l) in links.enumerated() {
            guard let a = screen(l.from), let b = screen(l.to) else { continue }
            let ab = b - a, len2 = simd_length_squared(ab)
            let t = len2 > 0 ? min(1, max(0, simd_dot(at - a, ab) / len2)) : 0
            let d = simd_distance(at, a + ab * t)
            if d <= limit, d < (best?.d ?? .infinity) { best = (k, d) }
        }
        return best?.k
    }
}
