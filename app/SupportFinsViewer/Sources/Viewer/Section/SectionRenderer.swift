import Metal
import simd

/// The section view's own pipelines: the cut-away half drawn again as glass (`ghost_fragment`), blended and
/// not writing depth, for the part (model frame) and for the supports (print frame).
@MainActor
struct SectionPipelines {
    let ghostMesh: MTLRenderPipelineState
    let ghostSupport: MTLRenderPipelineState

    init(device: MTLDevice, library lib: MTLLibrary) throws {
        func make(_ vertex: String) throws -> MTLRenderPipelineState {
            let d = MTLRenderPipelineDescriptor()
            d.vertexFunction = lib.makeFunction(name: vertex)
            d.fragmentFunction = lib.makeFunction(name: "ghost_fragment")
            let a = d.colorAttachments[0]!
            a.pixelFormat = ViewerRenderer.colorFormat
            a.isBlendingEnabled = true
            a.sourceRGBBlendFactor = .sourceAlpha
            a.destinationRGBBlendFactor = .oneMinusSourceAlpha
            a.sourceAlphaBlendFactor = .one
            a.destinationAlphaBlendFactor = .oneMinusSourceAlpha
            d.depthAttachmentPixelFormat = ViewerRenderer.depthFormat
            return try device.makeRenderPipelineState(descriptor: d)
        }
        ghostMesh = try make("mesh_vertex")
        ghostSupport = try make("support_vertex")
    }
}

extension ViewerRenderer {
    /// The cut as the shaders read it, or all zeros (no cut).
    var clipVector: SIMD4<Float> { section.isOn ? section.plane.clip : .zero }

    /// The posed parts' world-space box (`partBox`): the model frame's bounds, taken through the pose.
    var worldBounds: SectionBox? {
        guard !meshes.isEmpty else { return nil }
        let b = ViewerController.union(meshes.map(\.bounds))
        return SectionBox.world(of: b, model: modelMatrix)
    }

    /// After the scene: the cut-away half as glass if Ghost is on, the sheet and its rim, then the handle.
    func encodeSection(into enc: MTLRenderCommandEncoder) {
        guard section.isOn else { return }
        if section.ghost { encodeGhost(into: enc) }
        encodeSheet(into: enc)
        if let g = section.gizmo { encodeSectionGizmo(g, into: enc) }
    }

    /// `renderGhost`: the model only (the part and the supports), clipped to the other side, translucent, not
    /// writing depth, so the solid half still hides what is behind it and the glass never blocks a pick.
    private func encodeGhost(into enc: MTLRenderCommandEncoder) {
        var alpha = SectionTool.ghostOpacity
        let other = -clipVector
        enc.setDepthStencilState(overlayDepthState)
        enc.setFrontFacing(.counterClockwise)
        enc.setCullMode(.none)
        enc.setFragmentBytes(&alpha, length: MemoryLayout<Float>.size, index: 0)
        if !meshes.isEmpty {
            var u = uniforms()
            u.clip = other
            u.params.x = Self.noFace
            enc.setRenderPipelineState(sectionPipelines.ghostMesh)
            for (i, mesh) in meshes.enumerated() where mesh.faceCount > 0 {
                u.params.w = UInt32(faceBases[i])
                enc.setVertexBuffer(mesh.vertexBuffer, offset: 0, index: 0)
                enc.setVertexBuffer(flagBuffers[i], offset: 0, index: 1)
                enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
                enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: mesh.faceCount * 3)
            }
        }
        if let s = supports {
            var u = Uniforms(viewProj: camera.viewProjection, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1),
                             params: SIMD4(Self.noFace, UInt32(s.stride), 0, 0), clip: other)
            var c = SIMD4<Float>(Palette.fin, 1)
            enc.setRenderPipelineState(sectionPipelines.ghostSupport)
            func draw(_ buffer: MTLBuffer?, _ range: Range<Int>) {
                guard let buffer, !range.isEmpty else { return }
                enc.setVertexBuffer(buffer, offset: 0, index: 0)
                enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
                enc.setVertexBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
                enc.drawPrimitives(type: .triangle, vertexStart: range.lowerBound, vertexCount: range.count)
            }
            for seg in SupportSegments.fins(total: s.finVertices, hidden: hiddenFinRanges, crest: []) { draw(s.fins, seg.range) }
            draw(s.pad, 0..<s.padVertices)
        }
    }

    private static let handleBlue = SIMD3<Float>(0.302, 0.639, 1.0)     // 0x4da3ff

    /// The handle's translucent square and its outline (`sheet`, `rim`); lit while a press would grab it.
    private func encodeSheet(into enc: MTLRenderCommandEncoder) {
        let p = section.plane, h = section.sheetSize / 2
        let x = p.axisX * h, y = p.axisY * h, c = p.position
        let corners = [c - x - y, c + x - y, c + x + y, c - x + y]
        let order = [0, 1, 2, 0, 2, 3]
        let n = p.normal
        let verts = order.flatMap { i in [corners[i].x, corners[i].y, corners[i].z, n.x, n.y, n.z] }
        var u = Uniforms(viewProj: camera.viewProjection, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1),
                         params: SIMD4(Self.noFace, 6, 0, 0))
        var color = SIMD4<Float>(Self.handleBlue, section.hot ? SectionTool.sheetHotOpacity : SectionTool.sheetOpacity)
        enc.setRenderPipelineState(overlayPipeline)
        enc.setDepthStencilState(overlayDepthState)
        enc.setCullMode(.none)
        enc.setVertexBytes(verts, length: verts.count * 4, index: 0)
        enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
        enc.setVertexBytes(&color, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
        enc.setFragmentBytes(&color, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
        enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 6)
        stroke([corners[0], corners[1], corners[2], corners[3], corners[0]], color: Self.handleBlue, alpha: 0.7, width: 1,
               depth: lineDepthState, uniforms: &u, with: enc)
    }

    /// The arrow (blue), the two rings (red about X, green about Y) and the outer view ring (translucent yellow);
    /// the one hovered or dragged lights yellow and opaque (`TransformControlsGizmo`).
    private func encodeSectionGizmo(_ g: SectionGizmo, into enc: MTLRenderCommandEncoder) {
        var u = Uniforms(viewProj: camera.viewProjection, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1),
                         params: SIMD4(Self.noFace, 6, 0, 0))
        let current = section.dragging ?? section.hover
        let w = GizmoDisplay.widthPoints
        func lit(_ h: SectionHandle) -> Bool { current == h }
        stroke(g.circle(), color: GizmoDisplay.highlight, alpha: lit(.screen) ? 1 : GizmoDisplay.screenOpacity, width: w,
               depth: gizmoDepthState, uniforms: &u, with: enc)
        stroke(g.arc(.ringX), color: lit(.ringX) ? GizmoDisplay.highlight : SIMD3(1, 0, 0), alpha: 1, width: w,
               depth: gizmoDepthState, uniforms: &u, with: enc)
        stroke(g.arc(.ringY), color: lit(.ringY) ? GizmoDisplay.highlight : SIMD3(0, 1, 0), alpha: 1, width: w,
               depth: gizmoDepthState, uniforms: &u, with: enc)
        if g.showsArrow {
            let color: SIMD3<Float> = lit(.arrow) ? GizmoDisplay.highlight : SIMD3(0, 0, 1)
            stroke(g.shaft(), color: color, alpha: 1, width: w, depth: gizmoDepthState, uniforms: &u, with: enc)
            // the cones, pointing out at both ends
            for c in g.cones() {
                stroke([c.base, c.tip], color: color, alpha: 1, width: 3 * w, depth: gizmoDepthState, uniforms: &u, with: enc)
            }
        }
    }

    private func stroke(_ pts: [SIMD3<Float>], color: SIMD3<Float>, alpha: Float, width: Float, depth: MTLDepthStencilState,
                        uniforms u: inout Uniforms, with enc: MTLRenderCommandEncoder) {
        enc.setRenderPipelineState(ringPipeline)
        enc.setDepthStencilState(depth)
        enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
        var params = SIMD4<Float>(viewport.x, viewport.y, width * pixelScale, 0)
        enc.setVertexBytes(&params, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
        let flat = pts.flatMap { [$0.x, $0.y, $0.z] }
        var c = SIMD4<Float>(color, alpha)
        enc.setVertexBytes(flat, length: flat.count * 4, index: 0)
        enc.setFragmentBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
        enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: (pts.count - 1) * 6)
    }
}
