import Metal
import MetalKit
import QuartzCore
import simd

/// The Metal renderer: the plate, the part with its overhang colours, and the ID pass that picks a face.
/// It owns its camera and pose and draws from them every display frame; SwiftUI only sets them through
/// the methods below, so the render loop does not depend on SwiftUI state.
@MainActor
public final class ViewerRenderer: NSObject, MTKViewDelegate {
    public static let colorFormat = MTLPixelFormat.bgra8Unorm
    public static let depthFormat = MTLPixelFormat.depth32Float
    static let noFace: UInt32 = 0xFFFF_FFFF
    /// Above this many triangles the colour pass culls back faces.
    public static let cullAbove = 1_000_000

    struct Uniforms {
        var viewProj: simd_float4x4
        var model: simd_float4x4
        var eye: SIMD4<Float>
        var params: SIMD4<UInt32>
    }

    public let device: MTLDevice
    let queue: MTLCommandQueue
    let meshPipeline: MTLRenderPipelineState
    let idPipeline: MTLRenderPipelineState
    let linePipeline: MTLRenderPipelineState
    let supportPipeline: MTLRenderPipelineState
    let staleSupportPipeline: MTLRenderPipelineState
    let overlayPipeline: MTLRenderPipelineState
    let ringPipeline: MTLRenderPipelineState
    let gizmoDepthState: MTLDepthStencilState
    /// The load arrow: like the overlay, but through the model matrix.
    let hudPipeline: MTLRenderPipelineState
    let depthState: MTLDepthStencilState
    let lineDepthState: MTLDepthStencilState
    let overlayDepthState: MTLDepthStencilState
    /// No depth test and no depth write: the HUD (load arrow, layer frames) is always visible.
    let hudDepthState: MTLDepthStencilState

    public var camera = OrbitCamera.home() { didSet { changeCount += 1 } }
    /// Takes model coordinates to print coordinates (a pure rotation).
    public private(set) var rotation = matrix_identity_float3x3
    /// The engine's `Analysis.offset`: seats the rotated part on the plate.
    public private(set) var offset = SIMD3<Float>.zero
    public var highlightSmall = true { didSet { changeCount += 1 } }
    /// Draw every display frame even when nothing changed (the benchmark); otherwise an idle view draws nothing.
    public var continuous = false
    public private(set) var hoverFace: Int?
    /// The rotate rings, drawn over everything when set.
    public private(set) var gizmo: GizmoDisplay?
    /// Device pixels per point (ring thickness follows it); the view sets it.
    public var pixelScale: Float = 2
    private var viewport = SIMD2<Float>(1, 1)
    /// The parts on show: a file can hold several objects, all drawn in the file's frame.
    public private(set) var meshes: [any MeshSource] = []
    private var flagBuffers: [MTLBuffer] = []
    /// Keeps the parts' (large, `bytesNoCopy`) buffers resident on the GPU between frames.
    private var residency: MTLResidencySet?
    /// First global face index of each part, plus the total as the last element.
    public private(set) var faceBases: [Int] = [0]
    public var mesh: (any MeshSource)? { meshes.first }
    public var totalFaces: Int { faceBases.last ?? 0 }
    public private(set) var volume = PrinterVolume.standard
    public let stats = FrameStats()

    /// The fins on screen are the last settings', not the current ones: a rebuild is out (`markFinsStale`,
    /// `web/ui/finbuild.js:341`). They stay on screen at opacity 0.25, blended, so the part shows through.
    public static let staleOpacity: Float = 0.25
    public var dimSupports = false { didSet { if dimSupports != oldValue { changeCount += 1 } } }
    /// The fins and bed pad of the latest build (print space), or nil.
    public private(set) var supports: SupportLayer?
    /// Vertex ranges of the fin buffer not drawn (removed fins).
    public private(set) var hiddenFinRanges: [Range<Int>] = []
    /// Vertex ranges of the fin buffer drawn as interface material.
    public private(set) var crestFinRanges: [Range<Int>] = []
    /// Vertex ranges of the fin buffer under a removal click's pointer, drawn red over the fins.
    public private(set) var highlightedFinRanges: [Range<Int>] = []
    /// The load arrow (model space) and the layer frames (print space) of the Strength view; see `HUD.swift`.
    var loadArrow: (buffer: MTLBuffer, vertices: Int)?
    var layerFrames: (buffer: MTLBuffer, vertices: Int)?

    private var plate: PlateGeometry
    private var plateBuffers: [MTLBuffer?] = [nil, nil, nil, nil]
    private var idTexture: MTLTexture?
    private var idDepth: MTLTexture?
    private let readback: MTLBuffer
    private var lastDrawTime: Double?
    /// Bumped whenever something drawn changes; a test or a caller can see whether a frame was needed.
    public internal(set) var changeCount = 0
    private var drawnCount = -1

    public init?(device: MTLDevice? = MTLCreateSystemDefaultDevice()) {
        guard let device, let queue = device.makeCommandQueue() else { return nil }
        self.device = device
        self.queue = queue
        do {
            let lib = try device.makeLibrary(source: Shaders.source, options: nil)
            let color = MTLRenderPipelineDescriptor()
            color.vertexFunction = lib.makeFunction(name: "mesh_vertex")
            color.fragmentFunction = lib.makeFunction(name: "mesh_fragment")
            color.colorAttachments[0].pixelFormat = Self.colorFormat
            color.depthAttachmentPixelFormat = Self.depthFormat
            meshPipeline = try device.makeRenderPipelineState(descriptor: color)

            let id = MTLRenderPipelineDescriptor()
            id.vertexFunction = lib.makeFunction(name: "id_vertex")
            id.fragmentFunction = lib.makeFunction(name: "id_fragment")
            id.colorAttachments[0].pixelFormat = .r32Uint
            id.depthAttachmentPixelFormat = Self.depthFormat
            idPipeline = try device.makeRenderPipelineState(descriptor: id)

            let line = MTLRenderPipelineDescriptor()
            line.vertexFunction = lib.makeFunction(name: "line_vertex")
            line.fragmentFunction = lib.makeFunction(name: "line_fragment")
            let ca = line.colorAttachments[0]!
            ca.pixelFormat = Self.colorFormat
            ca.isBlendingEnabled = true
            ca.sourceRGBBlendFactor = .sourceAlpha
            ca.destinationRGBBlendFactor = .oneMinusSourceAlpha
            ca.sourceAlphaBlendFactor = .one
            ca.destinationAlphaBlendFactor = .oneMinusSourceAlpha
            line.depthAttachmentPixelFormat = Self.depthFormat
            linePipeline = try device.makeRenderPipelineState(descriptor: line)
            line.vertexFunction = lib.makeFunction(name: "ring_vertex")
            ringPipeline = try device.makeRenderPipelineState(descriptor: line)

            let support = MTLRenderPipelineDescriptor()
            support.vertexFunction = lib.makeFunction(name: "support_vertex")
            support.fragmentFunction = lib.makeFunction(name: "mesh_fragment")
            support.colorAttachments[0].pixelFormat = Self.colorFormat
            support.depthAttachmentPixelFormat = Self.depthFormat
            supportPipeline = try device.makeRenderPipelineState(descriptor: support)

            let stale = MTLRenderPipelineDescriptor()
            stale.vertexFunction = lib.makeFunction(name: "support_vertex")
            stale.fragmentFunction = lib.makeFunction(name: "support_stale_fragment")
            let sa = stale.colorAttachments[0]!
            sa.pixelFormat = Self.colorFormat
            sa.isBlendingEnabled = true
            sa.sourceRGBBlendFactor = .sourceAlpha
            sa.destinationRGBBlendFactor = .oneMinusSourceAlpha
            sa.sourceAlphaBlendFactor = .one
            sa.destinationAlphaBlendFactor = .oneMinusSourceAlpha
            stale.depthAttachmentPixelFormat = Self.depthFormat
            staleSupportPipeline = try device.makeRenderPipelineState(descriptor: stale)

            let overlay = MTLRenderPipelineDescriptor()
            overlay.vertexFunction = lib.makeFunction(name: "support_vertex")
            overlay.fragmentFunction = lib.makeFunction(name: "support_overlay_fragment")
            let oa = overlay.colorAttachments[0]!
            oa.pixelFormat = Self.colorFormat
            oa.isBlendingEnabled = true
            oa.sourceRGBBlendFactor = .sourceAlpha
            oa.destinationRGBBlendFactor = .oneMinusSourceAlpha
            oa.sourceAlphaBlendFactor = .one
            oa.destinationAlphaBlendFactor = .oneMinusSourceAlpha
            overlay.depthAttachmentPixelFormat = Self.depthFormat
            overlayPipeline = try device.makeRenderPipelineState(descriptor: overlay)
            overlay.vertexFunction = lib.makeFunction(name: "hud_vertex")
            hudPipeline = try device.makeRenderPipelineState(descriptor: overlay)
        } catch {
            return nil
        }
        let d = MTLDepthStencilDescriptor()
        d.depthCompareFunction = .less
        d.isDepthWriteEnabled = true
        depthState = device.makeDepthStencilState(descriptor: d)!
        let l = MTLDepthStencilDescriptor()
        l.depthCompareFunction = .lessEqual
        l.isDepthWriteEnabled = false
        lineDepthState = device.makeDepthStencilState(descriptor: l)!
        let o = MTLDepthStencilDescriptor()
        o.depthCompareFunction = .lessEqual
        o.isDepthWriteEnabled = false
        overlayDepthState = device.makeDepthStencilState(descriptor: o)!
        let gz = MTLDepthStencilDescriptor()
        gz.depthCompareFunction = .always
        gz.isDepthWriteEnabled = false
        gizmoDepthState = device.makeDepthStencilState(descriptor: gz)!
        let h = MTLDepthStencilDescriptor()
        h.depthCompareFunction = .always
        h.isDepthWriteEnabled = false
        hudDepthState = device.makeDepthStencilState(descriptor: h)!
        readback = device.makeBuffer(length: 4, options: .storageModeShared)!
        plate = PlateGeometry(volume: .standard)
        super.init()
        uploadPlate()
    }

    // MARK: supports

    /// Show the fins and pad of a build (nil clears them). The masks reset: a new build has new ranges.
    public func setSupports(_ layer: SupportLayer?) {
        supports = layer
        hiddenFinRanges = []
        crestFinRanges = []
        highlightedFinRanges = []
        rebuildResidency()
        changeCount += 1
    }

    /// Which fins are hidden (removed) and which vertices are interface material. See `SupportSegments`.
    public func setFinMask(hidden: [Range<Int>], crest: [Range<Int>]) {
        hiddenFinRanges = hidden
        crestFinRanges = crest
        changeCount += 1
    }

    public func setFinHighlight(_ ranges: [Range<Int>]) {
        guard ranges != highlightedFinRanges else { return }
        highlightedFinRanges = ranges
        changeCount += 1
    }

    // MARK: state

    public func setVolume(_ v: PrinterVolume) {
        guard v.isValid, v != volume else { return }
        volume = v
        plate = PlateGeometry(volume: v)
        uploadPlate()
        changeCount += 1
    }

    private func uploadPlate() {
        let lists = [plate.minor, plate.major, plate.edge, plate.box]
        plateBuffers = lists.map { pts in
            pts.isEmpty ? nil : device.makeBuffer(bytes: pts.map { [$0.x, $0.y, $0.z] as [Float] }.flatMap { $0 },
                                                  length: pts.count * 12, options: .storageModeShared)
        }
    }

    /// Show a part (nil clears it). The flags start empty; set them with `setFlags`.
    public func setMesh(_ m: (any MeshSource)?) { setMeshes(m.map { [$0] } ?? []) }

    /// Show the parts of a file. Faces are numbered across them, in order (`faceBases`).
    public func setMeshes(_ list: [any MeshSource]) {
        meshes = list
        flagBuffers = list.map { device.makeBuffer(length: max($0.faceCount, 1), options: .storageModeShared)! }
        var bases = [0]
        for m in list { bases.append(bases[bases.count - 1] + m.faceCount) }
        faceBases = bases
        hoverFace = nil
        rebuildResidency()
        changeCount += 1
    }

    private func rebuildResidency() {
        if let old = residency { queue.removeResidencySet(old) }
        residency = nil
        guard !meshes.isEmpty || supports != nil else { return }
        let d = MTLResidencySetDescriptor()
        d.initialCapacity = meshes.count * 2 + 2
        guard let set = try? device.makeResidencySet(descriptor: d) else { return }
        for m in meshes { set.addAllocation(m.vertexBuffer) }
        for f in flagBuffers { set.addAllocation(f) }
        if let b = supports?.fins { set.addAllocation(b) }
        if let b = supports?.pad { set.addAllocation(b) }
        set.commit()
        queue.addResidencySet(set)
        residency = set
    }

    public func setFlags(_ buffer: MTLBuffer) { setFlags(buffer, part: 0) }

    public func setFlags(_ buffer: MTLBuffer, part: Int) {
        guard part < flagBuffers.count else { return }
        flagBuffers[part] = buffer
        rebuildResidency()
        changeCount += 1
    }

    /// The part and the face within it for a global face index.
    public func locate(_ face: Int) -> (part: Int, face: Int)? {
        guard face >= 0, face < totalFaces else { return nil }
        let p = faceBases.lastIndex(where: { $0 <= face }) ?? 0
        return (min(p, meshes.count - 1), face - faceBases[min(p, meshes.count - 1)])
    }

    public func setPose(rotation: simd_float3x3, offset: SIMD3<Float>) {
        self.rotation = rotation
        self.offset = offset
        changeCount += 1
    }

    public func setGizmo(_ g: GizmoDisplay?) {
        guard g != gizmo else { return }
        gizmo = g
        changeCount += 1
    }

    public func setHover(_ face: Int?) {
        guard face != hoverFace else { return }
        hoverFace = face
        changeCount += 1
    }

    /// The flag byte of one face (for tests and the inspector).
    public func flagByte(_ face: Int) -> UInt8 {
        guard let (p, f) = locate(face), f < flagBuffers[p].length else { return 0 }
        return flagBuffers[p].contents().load(fromByteOffset: f, as: UInt8.self)
    }

    public var modelMatrix: simd_float4x4 {
        simd_float4x4(columns: (SIMD4(rotation.columns.0, 0), SIMD4(rotation.columns.1, 0),
                                SIMD4(rotation.columns.2, 0), SIMD4(offset, 1)))
    }

    private func uniforms() -> Uniforms {
        Uniforms(viewProj: camera.viewProjection, model: modelMatrix, eye: SIMD4(camera.eye, 1),
                 params: SIMD4(hoverFace.map { UInt32($0) } ?? Self.noFace,
                               UInt32((meshes.first?.vertexStride ?? 24) / 4), highlightSmall ? 1 : 0, 0))
    }

    // MARK: encoding

    private func draw(lines buffer: MTLBuffer?, count: Int, color: SIMD4<Float>, with enc: MTLRenderCommandEncoder) {
        guard let buffer else { return }
        var c = color
        enc.setVertexBuffer(buffer, offset: 0, index: 0)
        enc.setFragmentBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
        enc.drawPrimitives(type: .line, vertexStart: 0, vertexCount: count)
    }

    /// The colour pass: part first (depth-writing), then the plate lines over it.
    func encodeScene(into enc: MTLRenderCommandEncoder) {
        var u = uniforms()
        if !meshes.isEmpty {
            enc.setRenderPipelineState(meshPipeline)
            enc.setDepthStencilState(depthState)
            enc.setFrontFacing(.counterClockwise)
            // The web app draws both sides (the reverse dark, so an open mesh shows its holes). A big part
            // is drawn front faces only: half the triangles through the rasteriser keeps it at the display rate.
            enc.setCullMode(totalFaces > Self.cullAbove ? .back : .none)
            enc.setDepthBias(1, slopeScale: 1, clamp: 0)
            for (i, mesh) in meshes.enumerated() where mesh.faceCount > 0 {
                u.params.w = UInt32(faceBases[i])
                enc.setVertexBuffer(mesh.vertexBuffer, offset: 0, index: 0)
                enc.setVertexBuffer(flagBuffers[i], offset: 0, index: 1)
                enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
                enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: mesh.faceCount * 3)
            }
            enc.setDepthBias(0, slopeScale: 0, clamp: 0)
        }
        if let supports { encodeSupports(supports, into: enc) }
        u.params.w = 0
        enc.setRenderPipelineState(linePipeline)
        enc.setDepthStencilState(lineDepthState)
        enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
        let a = { (c: SIMD3<Float>, o: Float) in SIMD4(c, o) }
        draw(lines: plateBuffers[0], count: plate.minor.count, color: a(Palette.gridMinor, 0.9), with: enc)
        draw(lines: plateBuffers[1], count: plate.major.count, color: a(Palette.gridMajor, 1), with: enc)
        draw(lines: plateBuffers[2], count: plate.edge.count, color: a(Palette.plateEdge, 1), with: enc)
        draw(lines: plateBuffers[3], count: plate.box.count, color: a(Palette.volumeBox, 0.55), with: enc)
        if let g = gizmo, g.visible { drawGizmo(g, uniforms: &u, with: enc) }
        encodeHUD(into: enc)
    }

    private func drawGizmo(_ g: GizmoDisplay, uniforms u: inout Uniforms, with enc: MTLRenderCommandEncoder) {
        let rings = RotateGizmo(center: g.center)
        enc.setRenderPipelineState(ringPipeline)
        enc.setDepthStencilState(gizmoDepthState)
        enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
        var params = SIMD4<Float>(viewport.x, viewport.y, GizmoDisplay.widthPoints * pixelScale, 0)
        enc.setVertexBytes(&params, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
        for axis in GizmoAxis.allCases {
            // while a ring is being dragged only that one shows; otherwise all three
            if let active = g.active, active != axis { continue }
            let lit = g.active == axis || g.hover == axis
            let color = lit ? GizmoDisplay.highlight : axis.color
            let alpha = (lit ? 1 : GizmoDisplay.opacity) * g.fade
            let pts = rings.arc(axis, camera: camera).flatMap { [$0.x, $0.y, $0.z] }
            var c = SIMD4<Float>(color, alpha)
            enc.setVertexBytes(pts, length: pts.count * 4, index: 0)
            enc.setFragmentBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
            enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: RotateGizmo.segments * 6)
        }
    }

    /// The fins (minus the removed ones, the crest in its own colour), the pad, and the removal highlight.
    /// Supports are print-space: no model matrix. Both sides are drawn, as the web does.
    private func encodeSupports(_ s: SupportLayer, into enc: MTLRenderCommandEncoder) {
        var u = Uniforms(viewProj: camera.viewProjection, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1),
                         params: SIMD4(Self.noFace, UInt32(s.stride), 0, 0))
        enc.setRenderPipelineState(dimSupports ? staleSupportPipeline : supportPipeline)
        enc.setDepthStencilState(depthState)
        enc.setFrontFacing(.counterClockwise)
        enc.setCullMode(.none)
        var opacity = Self.staleOpacity
        if dimSupports { enc.setFragmentBytes(&opacity, length: MemoryLayout<Float>.size, index: 0) }
        func draw(_ buffer: MTLBuffer?, _ range: Range<Int>, _ color: SIMD3<Float>) {
            guard let buffer, !range.isEmpty else { return }
            var c = SIMD4(color, 1)
            enc.setVertexBuffer(buffer, offset: 0, index: 0)
            enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
            enc.setVertexBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
            enc.drawPrimitives(type: .triangle, vertexStart: range.lowerBound, vertexCount: range.count)
        }
        for seg in SupportSegments.fins(total: s.finVertices, hidden: hiddenFinRanges, crest: crestFinRanges) {
            draw(s.fins, seg.range, seg.kind == .crest ? Palette.crest : Palette.fin)
        }
        draw(s.pad, 0..<s.padVertices, Palette.pad)
        if !highlightedFinRanges.isEmpty {
            enc.setRenderPipelineState(overlayPipeline)
            enc.setDepthStencilState(overlayDepthState)
            enc.setDepthBias(-4, slopeScale: -4, clamp: 0)
            for r in SupportSegments.merged(highlightedFinRanges, within: s.finVertices) {
                guard let buffer = s.fins else { break }
                var c = SIMD4(Palette.removeHover, 0.55)
                enc.setVertexBuffer(buffer, offset: 0, index: 0)
                enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
                enc.setVertexBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 3)
                enc.setFragmentBytes(&c, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
                enc.drawPrimitives(type: .triangle, vertexStart: r.lowerBound, vertexCount: r.count)
            }
            enc.setDepthBias(0, slopeScale: 0, clamp: 0)
        }
    }

    func makeDepth(width: Int, height: Int) -> MTLTexture {
        let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: Self.depthFormat, width: width, height: height, mipmapped: false)
        d.usage = .renderTarget
        d.storageMode = .private
        return device.makeTexture(descriptor: d)!
    }

    // MARK: MTKViewDelegate

    public func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {
        if size.height > 0 { camera.aspect = Float(size.width / size.height) }
        changeCount += 1
    }

    public func draw(in view: MTKView) {
        if !continuous && drawnCount == changeCount { return }
        guard let pass = view.currentRenderPassDescriptor, let drawable = view.currentDrawable,
              let cb = queue.makeCommandBuffer() else { return }
        let bg = Palette.background
        pass.colorAttachments[0].clearColor = MTLClearColor(red: Double(bg.x), green: Double(bg.y), blue: Double(bg.z), alpha: 1)
        pass.colorAttachments[0].loadAction = .clear
        pass.depthAttachment.clearDepth = 1
        pass.depthAttachment.loadAction = .clear
        pass.depthAttachment.storeAction = .dontCare
        viewport = SIMD2(Float(drawable.texture.width), Float(drawable.texture.height))
        guard let enc = cb.makeRenderCommandEncoder(descriptor: pass) else { return }
        encodeScene(into: enc)
        enc.endEncoding()
        cb.present(drawable)
        drawnCount = changeCount
        let stats = self.stats
        cb.addCompletedHandler { cb in
            stats.record(presentedAt: cb.gpuEndTime, gpuSeconds: cb.gpuEndTime - cb.gpuStartTime)
        }
        cb.commit()
    }

    // MARK: offscreen

    /// Render the scene into a new `width x height` BGRA texture (shared, so the CPU can read it) and wait.
    public func renderOffscreen(width: Int, height: Int) -> MTLTexture {
        let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: Self.colorFormat, width: width, height: height, mipmapped: false)
        d.usage = [.renderTarget, .shaderRead]
        d.storageMode = .shared
        let color = device.makeTexture(descriptor: d)!
        let pass = MTLRenderPassDescriptor()
        let bg = Palette.background
        pass.colorAttachments[0].texture = color
        pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].storeAction = .store
        pass.colorAttachments[0].clearColor = MTLClearColor(red: Double(bg.x), green: Double(bg.y), blue: Double(bg.z), alpha: 1)
        pass.depthAttachment.texture = makeDepth(width: width, height: height)
        pass.depthAttachment.loadAction = .clear
        pass.depthAttachment.storeAction = .dontCare
        pass.depthAttachment.clearDepth = 1
        viewport = SIMD2(Float(width), Float(height))
        let cb = queue.makeCommandBuffer()!
        let enc = cb.makeRenderCommandEncoder(descriptor: pass)!
        encodeScene(into: enc)
        enc.endEncoding()
        cb.commit()
        cb.waitUntilCompleted()
        return color
    }

    /// BGRA bytes of a shared texture, row by row from the top.
    public static func pixels(of texture: MTLTexture) -> [UInt8] {
        var out = [UInt8](repeating: 0, count: texture.width * texture.height * 4)
        texture.getBytes(&out, bytesPerRow: texture.width * 4, from: MTLRegionMake2D(0, 0, texture.width, texture.height), mipmapLevel: 0)
        return out
    }

    // MARK: picking

    /// The face under pixel (`x`, `y`) of a `width x height` view (origin top left), or nil: an ID pass
    /// renders each face's index into an R32Uint texture, and one pixel of it is read back.
    public func pickFace(x: Int, y: Int, width: Int, height: Int) async -> Int? {
        guard totalFaces > 0, x >= 0, y >= 0, x < width, y < height else { return nil }
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
        var u = uniforms()
        let enc = cb.makeRenderCommandEncoder(descriptor: pass)!
        enc.setRenderPipelineState(idPipeline)
        enc.setDepthStencilState(depthState)
        for (i, mesh) in meshes.enumerated() where mesh.faceCount > 0 {
            u.params.w = UInt32(faceBases[i])
            enc.setVertexBuffer(mesh.vertexBuffer, offset: 0, index: 0)
            enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
            enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: mesh.faceCount * 3)
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
        return v == 0 ? nil : Int(v - 1)
    }

    // MARK: picking supports

    /// The triangle of the fin buffer under pixel (`x`, `y`) of a `width x height` view (origin top left), or
    /// nil. Only the fins that are drawn can be picked: the removed ones (`hiddenFinRanges`) are not in the ID
    /// pass, and the pad is never a candidate. The answer is the triangle index into the fin buffer
    /// (vertex `3 * triangle`), which `triRanges` map to a fin (`web/ui/remove.js` pickFin).
    public func pickSupport(x: Int, y: Int, width: Int, height: Int) async -> Int? {
        guard let s = supports, s.finVertices > 0, let fins = s.fins, x >= 0, y >= 0, x < width, y < height else { return nil }
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
        // the part is in the pass as an occluder (a fin behind the part cannot be clicked), with no id of its own
        var u = Uniforms(viewProj: camera.viewProjection, model: matrix_identity_float4x4, eye: SIMD4(camera.eye, 1),
                         params: SIMD4(Self.noFace, UInt32(s.stride), 0, 0))
        let enc = cb.makeRenderCommandEncoder(descriptor: pass)!
        enc.setRenderPipelineState(idPipeline)
        enc.setDepthStencilState(depthState)
        for seg in SupportSegments.fins(total: s.finVertices, hidden: hiddenFinRanges, crest: []) {
            enc.setVertexBuffer(fins, offset: 0, index: 0)
            enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
            enc.drawPrimitives(type: .triangle, vertexStart: seg.range.lowerBound, vertexCount: seg.range.count)
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
        return v == 0 ? nil : Int(v - 1)
    }
}


/// How the rotate rings are shown (see `RotateGizmo`).
public struct GizmoDisplay: Equatable, Sendable {
    /// three.js' hover colour (`0xffff00`).
    public static let highlight = SIMD3<Float>(1, 1, 0)
    public static let opacity: Float = 0.9
    public static let widthPoints: Float = 3

    public var center: SIMD3<Float>
    public var visible = true
    public var hover: GizmoAxis?
    public var active: GizmoAxis?
    /// 1 normally; the rings go faint while a sway brace is aimed (`web/ui/ringfade.js`, `FADED` = 0.15).
    public var fade: Float = 1

    public init(center: SIMD3<Float>, visible: Bool = true, hover: GizmoAxis? = nil, active: GizmoAxis? = nil, fade: Float = 1) {
        self.center = center
        self.visible = visible
        self.hover = hover
        self.active = active
        self.fade = fade
    }
}
