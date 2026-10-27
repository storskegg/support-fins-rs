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
    let depthState: MTLDepthStencilState
    let lineDepthState: MTLDepthStencilState

    public var camera = OrbitCamera.home() { didSet { changeCount += 1 } }
    /// Takes model coordinates to print coordinates (a pure rotation).
    public private(set) var rotation = matrix_identity_float3x3
    /// The engine's `Analysis.offset`: seats the rotated part on the plate.
    public private(set) var offset = SIMD3<Float>.zero
    public var highlightSmall = true { didSet { changeCount += 1 } }
    /// Draw every display frame even when nothing changed (the benchmark); otherwise an idle view draws nothing.
    public var continuous = false
    public private(set) var hoverFace: Int?
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

    private var plate: PlateGeometry
    private var plateBuffers: [MTLBuffer?] = [nil, nil, nil, nil]
    private var idTexture: MTLTexture?
    private var idDepth: MTLTexture?
    private let readback: MTLBuffer
    private var lastDrawTime: Double?
    /// Bumped whenever something drawn changes; a test or a caller can see whether a frame was needed.
    public private(set) var changeCount = 0
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
        readback = device.makeBuffer(length: 4, options: .storageModeShared)!
        plate = PlateGeometry(volume: .standard)
        super.init()
        uploadPlate()
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
        guard !meshes.isEmpty else { return }
        let d = MTLResidencySetDescriptor()
        d.initialCapacity = meshes.count * 2
        guard let set = try? device.makeResidencySet(descriptor: d) else { return }
        for m in meshes { set.addAllocation(m.vertexBuffer) }
        for f in flagBuffers { set.addAllocation(f) }
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
        u.params.w = 0
        enc.setRenderPipelineState(linePipeline)
        enc.setDepthStencilState(lineDepthState)
        enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.size, index: 2)
        let a = { (c: SIMD3<Float>, o: Float) in SIMD4(c, o) }
        draw(lines: plateBuffers[0], count: plate.minor.count, color: a(Palette.gridMinor, 0.9), with: enc)
        draw(lines: plateBuffers[1], count: plate.major.count, color: a(Palette.gridMajor, 1), with: enc)
        draw(lines: plateBuffers[2], count: plate.edge.count, color: a(Palette.plateEdge, 1), with: enc)
        draw(lines: plateBuffers[3], count: plate.box.count, color: a(Palette.volumeBox, 0.55), with: enc)
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
}
