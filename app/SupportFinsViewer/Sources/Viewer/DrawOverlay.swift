import Metal
import simd

/// Triangles in print space as a Metal buffer: interleaved position + normal, `f32`, six floats a vertex, three
/// vertices a triangle (`GpuBuffer`'s layout, wrapped with no copy).
public struct OverlayMesh: @unchecked Sendable {
    public var buffer: MTLBuffer
    public var vertices: Int
    public init(buffer: MTLBuffer, vertices: Int) { (self.buffer, self.vertices) = (buffer, vertices) }
}

/// A dot that keeps its size on screen: a sphere of `radiusPixels` CSS pixels at `position`, whatever the zoom
/// (`sizeMarkers`, `web/ui/walls.js:107-115`).
public struct OverlayMarker: Equatable, Sendable {
    public var position: SIMD3<Float>
    public var radiusPixels: Float
    public var color: SIMD3<Float>
    public var alpha: Float
    public init(position: SIMD3<Float>, radiusPixels: Float, color: SIMD3<Float>, alpha: Float) {
        (self.position, self.radiusPixels, self.color, self.alpha) = (position, radiusPixels, color, alpha)
    }
}

/// What Draw puts over the part (`web/ui/walls.js`, `web/ui/braceghost.js`): the hand-placed supports, the one
/// selected, the ghost of the wall in progress, the ghost of a sway brace and its fade, the dots and the band.
public struct DrawScene: @unchecked Sendable {
    /// The committed hand-placed walls and braces (`drawMaterial`, green).
    public var walls: OverlayMesh?
    /// The selected one, amber over the wall (`selMaterial`).
    public var selection: OverlayMesh?
    /// The wall in progress, translucent (`ghostMaterial`, opacity 0.45).
    public var ghost: OverlayMesh?
    /// Sway brace ghosts, each at its own opacity (the fade of `braceghost.js`).
    public var braces: [(mesh: OverlayMesh, opacity: Float)] = []
    public var markers: [OverlayMarker] = []
    /// The rubber band: the straight line, or a curve's arc ending at the cursor.
    public var band: [SIMD3<Float>] = []
    /// The band turns red when the wall in progress cannot build (`BAND_BAD`).
    public var bandBad = false
    /// The supports lag the pose or a rebuild is out: drawn at opacity 0.25 (`markFinsStale`).
    public var dim = false

    public init() {}

    public var isEmpty: Bool {
        walls == nil && selection == nil && ghost == nil && braces.isEmpty && markers.isEmpty && band.isEmpty
    }
}

/// The colours of Draw (`web/ui/walls.js:50-55, 76-80`, `web/ui/braceghost.js:22`).
public enum DrawPalette {
    /// `drawMaterial` is the fin colour, `0x59d98e`.
    public static let wall = Palette.fin
    public static let ghost = Palette.hex(0x8ff0bd)
    public static let selection = Palette.hex(0xffb347)
    public static let dot = Palette.hex(0x59d98e)
    public static let cursor = Palette.hex(0xcffbe4)
    /// Eased to on a spot that cannot take a brace (`CURSOR_BAD`).
    public static let cursorBad = Palette.hex(0xffb454)
    public static let bandOK = Palette.hex(0x6dffab)
    public static let bandBad = Palette.hex(0xff6b6b)
    public static let ghostOpacity: Float = 0.45
    public static let cursorOpacity: Float = 0.6
    /// `DOT_PX`, `CURSOR_PX`: marker radii in CSS pixels.
    public static let dotPixels: Float = 5
    public static let cursorPixels: Float = 4
}

/// The renderer's state for Draw (kept in one object so the renderer class gains one property).
@MainActor
final class DrawRenderState {
    var scene: DrawScene?
    /// A unit sphere (`SphereGeometry(1, 20, 14)`), interleaved position + normal.
    var sphere: (buffer: MTLBuffer, vertices: Int)?
}

extension ViewerRenderer {
    /// Show the Draw layers (nil takes them off).
    public func setDrawScene(_ scene: DrawScene?) {
        if scene == nil && drawState.scene == nil { return }
        drawState.scene = scene
        changeCount += 1
    }

    public var drawScene: DrawScene? { drawState.scene }

    /// World mm per CSS pixel at `p`, for the perspective camera (`sizeMarkers`'s `mmPerPx`).
    func millimetersPerPixel(at p: SIMD3<Float>) -> Float {
        2 * tan(camera.fovY / 2) * simd_distance(camera.eye, p) / max(1, viewport.y / max(pixelScale, 1))
    }

    /// The layers, over the part and the supports: walls and ghosts depth-tested, the dots and the band over
    /// everything (a HUD: `depthTest: false`, `renderOrder` 11).
    func encodeDrawScene(into enc: MTLRenderCommandEncoder) {
        guard let s = drawState.scene else { return }
        var u = Uniforms(viewProj: camera.viewProjection, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1),
                         params: SIMD4(Self.noFace, 6, 0, 0))
        enc.setFrontFacing(.counterClockwise)
        enc.setCullMode(.none)
        func lit(_ mesh: OverlayMesh, color: SIMD3<Float>, opacity: Float?) {
            enc.setRenderPipelineState(opacity == nil ? supportPipeline : staleSupportPipeline)
            if var o = opacity { enc.setFragmentBytes(&o, length: MemoryLayout<Float>.size, index: 0) }
            var c = SIMD4(color, 1)
            enc.setVertexBuffer(mesh.buffer, offset: 0, index: 0)
            enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
            enc.setVertexBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
            enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: mesh.vertices)
        }
        enc.setDepthStencilState(depthState)
        if let w = s.walls { lit(w, color: DrawPalette.wall, opacity: s.dim ? Self.staleOpacity : nil) }
        if let sel = s.selection {
            // over the wall it highlights (`polygonOffset -1`)
            enc.setDepthStencilState(overlayDepthState)
            enc.setDepthBias(-4, slopeScale: -4, clamp: 0)
            lit(sel, color: DrawPalette.selection, opacity: s.dim ? Self.staleOpacity : nil)
            enc.setDepthBias(0, slopeScale: 0, clamp: 0)
            enc.setDepthStencilState(depthState)
        }
        if let g = s.ghost { lit(g, color: DrawPalette.ghost, opacity: DrawPalette.ghostOpacity) }
        for b in s.braces where b.opacity > 0 { lit(b.mesh, color: DrawPalette.ghost, opacity: b.opacity) }

        enc.setDepthStencilState(hudDepthState)
        if s.band.count >= 2 {
            var c = SIMD4(s.bandBad ? DrawPalette.bandBad : DrawPalette.bandOK, 1)
            let flat = s.band.flatMap { [$0.x, $0.y, $0.z] }
            enc.setRenderPipelineState(linePipeline)
            enc.setVertexBytes(flat, length: flat.count * 4, index: 0)
            enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
            enc.setFragmentBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
            enc.drawPrimitives(type: .lineStrip, vertexStart: 0, vertexCount: s.band.count)
        }
        guard !s.markers.isEmpty else { return }
        if drawState.sphere == nil { drawState.sphere = makeUnitSphere() }
        guard let sphere = drawState.sphere else { return }
        enc.setRenderPipelineState(hudPipeline)
        for m in s.markers {
            let r = m.radiusPixels * millimetersPerPixel(at: m.position)
            var mu = u
            mu.model = simd_float4x4(columns: (SIMD4(r, 0, 0, 0), SIMD4(0, r, 0, 0), SIMD4(0, 0, r, 0), SIMD4(m.position, 1)))
            var c = SIMD4(m.color, m.alpha)
            enc.setVertexBuffer(sphere.buffer, offset: 0, index: 0)
            enc.setVertexBytes(&mu, length: MemoryLayout<Uniforms>.size, index: 2)
            enc.setVertexBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
            enc.setFragmentBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
            enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: sphere.vertices)
        }
    }

    /// `SphereGeometry(1, 20, 14)`: 20 segments round, 14 rings, as triangles with position + normal.
    private func makeUnitSphere() -> (MTLBuffer, Int)? {
        let w = 20, h = 14
        func p(_ ix: Int, _ iy: Int) -> SIMD3<Float> {
            let u = Float(ix) / Float(w), v = Float(iy) / Float(h)
            return SIMD3(-cos(u * 2 * .pi) * sin(v * .pi), cos(v * .pi), sin(u * 2 * .pi) * sin(v * .pi))
        }
        var out: [Float] = []
        func vertex(_ q: SIMD3<Float>) { out += [q.x, q.y, q.z, q.x, q.y, q.z] }
        for iy in 0..<h {
            for ix in 0..<w {
                let a = p(ix, iy + 1), b = p(ix, iy), c = p(ix + 1, iy), d = p(ix + 1, iy + 1)
                if iy != 0 { vertex(a); vertex(b); vertex(d) }
                if iy != h - 1 { vertex(b); vertex(c); vertex(d) }
            }
        }
        guard let buffer = device.makeBuffer(bytes: out, length: out.count * 4, options: .storageModeShared) else { return nil }
        return (buffer, out.count / 6)
    }
}
