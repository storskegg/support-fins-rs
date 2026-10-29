import Metal
import simd

/// A model standing on the plate beside the live one (a parked model: `ui/plateview.js` draws it where it
/// stands, its fins as they were left, dimmed). The renderer only draws it; the app decides what is parked and
/// where (`Plates/`).
@MainActor
public struct ParkedDrawable {
    public var mesh: any MeshSource
    /// One byte per face (`FaceFlags`), for the pose it was parked in: it keeps its overhang colours.
    public var flags: MTLBuffer
    /// Takes model coordinates to print coordinates, and the seating offset: as `ViewerRenderer.rotation` / `offset`
    /// are for the live part.
    public var rotation: simd_float3x3
    public var offset: SIMD3<Float>
    /// Where its spot is in the scene: the plate's origin plus the model's spot on it, less the live model's.
    public var origin: SIMD2<Float>
    /// Its fins and pad (print space, about its own spot), drawn in the ghost fin colour; `hidden` are the removed
    /// fins' vertex ranges.
    public var supports: SupportLayer?
    public var hiddenFins: [Range<Int>]
    /// Built under settings the plate has since changed: greyed until rebuilt (`ghostLook`).
    public var stale: Bool

    public init(mesh: any MeshSource, flags: MTLBuffer, rotation: simd_float3x3, offset: SIMD3<Float>, origin: SIMD2<Float>,
                supports: SupportLayer? = nil, hiddenFins: [Range<Int>] = [], stale: Bool = false) {
        (self.mesh, self.flags, self.rotation, self.offset, self.origin) = (mesh, flags, rotation, offset, origin)
        (self.supports, self.hiddenFins, self.stale) = (supports, hiddenFins, stale)
    }

    var matrix: simd_float4x4 {
        simd_float4x4(translation: SIMD3(origin, 0)) * simd_float4x4(columns: (
            SIMD4(rotation.columns.0, 0), SIMD4(rotation.columns.1, 0), SIMD4(rotation.columns.2, 0), SIMD4(offset, 1)))
    }
}

/// Everything the plates add to the scene (`ui/plateview.js`): the beds of the row, the parked models and the
/// clash cells. All in the scene's coordinates, where the live model stands at the origin.
@MainActor
public struct PlateScene {
    /// The scene origin of each bed: the plate being edited and, when there are several, the others along X.
    public var beds: [SIMD2<Float>] = [.zero]
    /// The bed of the plate being edited, the one that also draws the build-volume box.
    public var edited = 0
    /// A bed lit as a drop's target (a dragged model over another plate), or nil.
    public var target: Int?
    public var parked: [ParkedDrawable] = []
    /// Cells models of the plate being edited share, as scene corners of 1 mm cells (`showClash`).
    public var clash: [SIMD2<Float>] = []

    public init() {}

    public init(beds: [SIMD2<Float>], edited: Int, target: Int? = nil, parked: [ParkedDrawable] = [], clash: [SIMD2<Float>] = []) {
        (self.beds, self.edited, self.target, self.parked, self.clash) = (beds, edited, target, parked, clash)
    }
}

extension simd_float4x4 {
    init(translation t: SIMD3<Float>) {
        self.init(columns: (SIMD4(1, 0, 0, 0), SIMD4(0, 1, 0, 0), SIMD4(0, 0, 1, 0), SIMD4(t, 1)))
    }
}

extension ViewerRenderer {
    /// The ghost fin colour (`ghostFin`, `ui/plateview.js`): darker than the live fins.
    static let ghostFin = Palette.hex(0x3d8a64)
    /// The clash cells (`clashMat`, `ui/plateview.js`).
    static let clashColor = SIMD4<Float>(Palette.hex(0xff3b30), 0.6)
    /// The lit target bed (`target`, `ui/plateview.js`).
    static let targetColor = SIMD4<Float>(Palette.hex(0x4da3ff), 0.18)

    /// Show these plates: replaces the beds, the parked models and the clash.
    public func setPlateScene(_ scene: PlateScene) {
        plateScene = scene
        rebuildResidency()
        clashBuffer = nil
        let cells = scene.clash
        if !cells.isEmpty {
            // two triangles a cell at z 0.1 (`showClash`), position + normal like every other buffer
            var data: [Float] = []
            data.reserveCapacity(cells.count * 36)
            for c in cells {
                let p: [SIMD2<Float>] = [c, c + [1, 0], c + [1, 1], c, c + [1, 1], c + [0, 1]]
                for q in p { data += [q.x, q.y, 0.1, 0, 0, 1] }
            }
            clashBuffer = (device.makeBuffer(bytes: data, length: data.count * 4, options: .storageModeShared)!, cells.count * 6)
        }
        changeCount += 1
    }

    /// The parked models, drawn with the ghost look, after the live part.
    func encodeParked(_ viewProj: simd_float4x4, into enc: MTLRenderCommandEncoder) {
        let list = plateScene.parked.filter { $0.mesh.faceCount > 0 }
        guard !list.isEmpty else { return }
        enc.setRenderPipelineState(meshPipeline)
        enc.setDepthStencilState(depthState)
        enc.setFrontFacing(.counterClockwise)
        enc.setCullMode(totalFaces > Self.cullAbove ? .back : .none)
        enc.setDepthBias(1, slopeScale: 1, clamp: 0)
        for p in list {
            // params.z bit 1: the ghost tint of a parked part (`ghostPart`)
            var u = Uniforms(viewProj: viewProj, model: p.matrix, eye: SIMD4(camera.eye, 1),
                             params: SIMD4(Self.noFace, UInt32(p.mesh.vertexStride / 4), (highlightSmall ? 1 : 0) | 2, 0))
            enc.setVertexBuffer(p.mesh.vertexBuffer, offset: 0, index: 0)
            enc.setVertexBuffer(p.flags, offset: 0, index: 1)
            enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
            enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: p.mesh.faceCount * 3)
        }
        enc.setDepthBias(0, slopeScale: 0, clamp: 0)
        // their fins and pads: one flat ghost colour, print space about the model's own spot
        for p in list {
            guard let s = p.supports else { continue }
            var u = Uniforms(viewProj: viewProj * simd_float4x4(translation: SIMD3(p.origin, 0)), model: matrix_identity_float4x4,
                             eye: SIMD4(camera.eye, 1), params: SIMD4(Self.noFace, UInt32(s.stride), 0, 0))
            enc.setRenderPipelineState(p.stale ? staleSupportPipeline : supportPipeline)
            enc.setDepthStencilState(depthState)
            enc.setCullMode(.none)
            var opacity = Self.staleOpacity
            if p.stale { enc.setFragmentBytes(&opacity, length: MemoryLayout<Float>.size, index: 0) }
            func draw(_ buffer: MTLBuffer?, _ range: Range<Int>) {
                guard let buffer, !range.isEmpty else { return }
                var c = SIMD4(Self.ghostFin, 1)
                enc.setVertexBuffer(buffer, offset: 0, index: 0)
                enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
                enc.setVertexBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
                enc.drawPrimitives(type: .triangle, vertexStart: range.lowerBound, vertexCount: range.count)
            }
            for seg in SupportSegments.fins(total: s.finVertices, hidden: p.hiddenFins, crest: []) { draw(s.fins, seg.range) }
            draw(s.pad, 0..<s.padVertices)
        }
    }

    /// The red clash cells, drawn through everything (`clashMat`: depth test off).
    func encodeClash(_ viewProj: simd_float4x4, into enc: MTLRenderCommandEncoder) {
        guard let (buffer, count) = clashBuffer else { return }
        var u = Uniforms(viewProj: viewProj, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1), params: SIMD4(Self.noFace, 6, 0, 0))
        var c = Self.clashColor
        enc.setRenderPipelineState(overlayPipeline)
        enc.setDepthStencilState(hudDepthState)
        enc.setCullMode(.none)
        enc.setVertexBuffer(buffer, offset: 0, index: 0)
        enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
        enc.setVertexBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
        enc.setFragmentBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
        enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: count)
    }

    /// The lit target bed: a translucent sheet over it (`target`, `ui/plateview.js`).
    func encodeTarget(_ viewProj: simd_float4x4, into enc: MTLRenderCommandEncoder) {
        guard let i = plateScene.target, i < plateScene.beds.count else { return }
        let o = plateScene.beds[i], hx = volume.x / 2, hy = volume.y / 2
        let p: [SIMD2<Float>] = [[-hx, -hy], [hx, -hy], [hx, hy], [-hx, -hy], [hx, hy], [-hx, hy]]
        var data: [Float] = []
        for q in p { data += [o.x + q.x, o.y + q.y, 0.05, 0, 0, 1] }
        var u = Uniforms(viewProj: viewProj, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1), params: SIMD4(Self.noFace, 6, 0, 0))
        var c = Self.targetColor
        enc.setRenderPipelineState(overlayPipeline)
        enc.setDepthStencilState(overlayDepthState)
        enc.setCullMode(.none)
        enc.setVertexBytes(data, length: data.count * 4, index: 0)
        enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
        enc.setVertexBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
        enc.setFragmentBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
        enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 6)
    }
}

/// What a pick on the plates lands on (`plateAt` / `parkedAt`, `ui/plateview.js`).
public enum ModelPick: Equatable, Sendable {
    /// The live model, its fins or its pad: that click is for the model being edited.
    case live
    /// A parked model of the plate being edited, by its index in `PlateScene.parked`.
    case parked(Int)
}

extension ViewerRenderer {
    /// Which model is under pixel (`x`, `y`) of a `width x height` view (origin top left): the live part and
    /// its fins, or one of the parked models, or nil for the bed or the background. The nearest wins.
    public func pickModel(x: Int, y: Int, width: Int, height: Int) async -> ModelPick? {
        guard x >= 0, y >= 0, x < width, y < height else { return nil }
        if idTexture?.width != width || idTexture?.height != height {
            let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .r32Uint, width: width, height: height, mipmapped: false)
            d.usage = [.renderTarget]
            d.storageMode = .private
            idTexture = device.makeTexture(descriptor: d)
            idDepth = makeDepth(width: width, height: height)
        }
        guard let idTexture, let idDepth, let cb = queue.makeCommandBuffer() else { return nil }
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = idTexture
        pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].storeAction = .store
        pass.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 0)
        pass.depthAttachment.texture = idDepth
        pass.depthAttachment.loadAction = .clear
        pass.depthAttachment.storeAction = .dontCare
        pass.depthAttachment.clearDepth = 1
        guard let enc = cb.makeRenderCommandEncoder(descriptor: pass) else { return nil }
        enc.setRenderPipelineState(idFlatPipeline)
        enc.setDepthStencilState(depthState)
        // ids are the model's number plus one in the result (0 = nothing): live is id 0 here, parked i is i + 1
        func draw(_ buffer: MTLBuffer, stride: Int, vertices: Int, model: simd_float4x4, viewProj: simd_float4x4, id: UInt32) {
            var u = Uniforms(viewProj: viewProj, model: model, eye: SIMD4(camera.eye, 1), params: SIMD4(Self.noFace, UInt32(stride), 0, id))
            enc.setVertexBuffer(buffer, offset: 0, index: 0)
            enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
            enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: vertices)
        }
        let vp = camera.viewProjection
        for mesh in meshes where mesh.faceCount > 0 {
            draw(mesh.vertexBuffer, stride: mesh.vertexStride / 4, vertices: mesh.faceCount * 3, model: modelMatrix, viewProj: vp, id: 0)
        }
        if let s = supports {
            for seg in SupportSegments.fins(total: s.finVertices, hidden: hiddenFinRanges, crest: []) {
                guard let fins = s.fins else { break }
                var u = Uniforms(viewProj: vp, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1), params: SIMD4(Self.noFace, UInt32(s.stride), 0, 0))
                enc.setVertexBuffer(fins, offset: 0, index: 0)
                enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
                enc.drawPrimitives(type: .triangle, vertexStart: seg.range.lowerBound, vertexCount: seg.range.count)
            }
            if let pad = s.pad, s.padVertices > 0 {
                var u = Uniforms(viewProj: vp, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1), params: SIMD4(Self.noFace, UInt32(s.stride), 0, 0))
                enc.setVertexBuffer(pad, offset: 0, index: 0)
                enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
                enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: s.padVertices)
            }
        }
        for (i, p) in plateScene.parked.enumerated() {
            if p.mesh.faceCount > 0 {
                draw(p.mesh.vertexBuffer, stride: p.mesh.vertexStride / 4, vertices: p.mesh.faceCount * 3, model: p.matrix, viewProj: vp, id: UInt32(i + 1))
            }
            if let s = p.supports {
                let moved = vp * simd_float4x4(translation: SIMD3(p.origin, 0))
                for seg in SupportSegments.fins(total: s.finVertices, hidden: p.hiddenFins, crest: []) {
                    guard let fins = s.fins else { break }
                    var u = Uniforms(viewProj: moved, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1), params: SIMD4(Self.noFace, UInt32(s.stride), 0, UInt32(i + 1)))
                    enc.setVertexBuffer(fins, offset: 0, index: 0)
                    enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
                    enc.drawPrimitives(type: .triangle, vertexStart: seg.range.lowerBound, vertexCount: seg.range.count)
                }
                if let pad = s.pad, s.padVertices > 0 {
                    var u = Uniforms(viewProj: moved, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1), params: SIMD4(Self.noFace, UInt32(s.stride), 0, UInt32(i + 1)))
                    enc.setVertexBuffer(pad, offset: 0, index: 0)
                    enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
                    enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: s.padVertices)
                }
            }
        }
        enc.endEncoding()
        let blit = cb.makeBlitCommandEncoder()!
        blit.copy(from: idTexture, sourceSlice: 0, sourceLevel: 0, sourceOrigin: MTLOrigin(x: x, y: y, z: 0),
                  sourceSize: MTLSize(width: 1, height: 1, depth: 1), to: readback, destinationOffset: 0,
                  destinationBytesPerRow: 4, destinationBytesPerImage: 4)
        blit.endEncoding()
        await withCheckedContinuation { (k: CheckedContinuation<Void, Never>) in
            cb.addCompletedHandler { _ in k.resume() }
            cb.commit()
        }
        let v = readback.contents().load(as: UInt32.self)
        // the fragment writes id + 1: 1 is the live model, 2... the parked ones
        return v == 0 ? nil : (v == 1 ? .live : .parked(Int(v) - 2))
    }

    /// The point of the plate (z = 0) under pixel (`x`, `y`), in scene coordinates, or nil when the ray misses it.
    public func groundPoint(x: Int, y: Int, width: Int, height: Int) -> SIMD2<Float>? {
        guard width > 0, height > 0 else { return nil }
        let r = camera.ray(ndcX: 2 * Float(x) / Float(width) - 1, ndcY: 1 - 2 * Float(y) / Float(height))
        guard abs(r.direction.z) > 1e-6 else { return nil }
        let t = -r.origin.z / r.direction.z
        guard t > 0 else { return nil }
        let p = r.origin + r.direction * t
        return SIMD2(p.x, p.y)
    }
}
