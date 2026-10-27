import Metal
import MetalKit
import Observation
import simd

/// What the UI binds to: the settings of the view (threshold, pose, volume), the hover, and the renderer they
/// drive. Setting a property updates the renderer at once; nothing SwiftUI does is on the render path.
@MainActor
@Observable
public final class ViewerController {
    public let renderer: ViewerRenderer

    public private(set) var meshName = ""
    public private(set) var faceCount = 0
    public private(set) var size = SIMD3<Float>.zero
    public private(set) var hoverFace: Int?
    /// The part and face of the hover.
    public var hoverLocation: (part: Int, face: Int)? { hoverFace.flatMap { renderer.locate($0) } }

    public var thresholdDegrees: Double = 45 { didSet { if thresholdDegrees != oldValue { refreshFlags() } } }
    public var rotationDegrees = SIMD3<Float>.zero { didSet { if rotationDegrees != oldValue { refreshPose() } } }
    public var volume = PrinterVolume.standard { didSet { renderer.setVolume(volume) } }
    public var highlightSmall = true { didSet { renderer.highlightSmall = highlightSmall } }

    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var pickInFlight = false
    @ObservationIgnored private var pendingPick: (x: Int, y: Int, w: Int, h: Int)?
    @ObservationIgnored private var benchmarkLink: Task<Void, Never>?

    public init?(device: MTLDevice? = MTLCreateSystemDefaultDevice()) {
        guard let r = ViewerRenderer(device: device) else { return nil }
        renderer = r
        r.setVolume(volume)
    }

    // MARK: the part

    public func show(_ mesh: any MeshSource, name: String) { show([mesh], name: name) }

    /// Show the parts of one file, drawn where the file puts them. The pose (rotation and seating) is the
    /// whole file's: the union of the parts' bounds, turned about its middle.
    public func show(_ parts: [any MeshSource], name: String) {
        renderer.setMeshes(parts)
        meshName = name
        faceCount = renderer.totalFaces
        partCount = parts.count
        let b = Self.union(parts.map(\.bounds))
        groupBounds = b
        size = b.max - b.min
        renderer.camera.frame(size: size)
        refreshPose()
    }

    public private(set) var partCount = 0
    @ObservationIgnored private var groupBounds = (min: SIMD3<Float>.zero, max: SIMD3<Float>.zero)

    static func union(_ list: [(min: SIMD3<Float>, max: SIMD3<Float>)]) -> (min: SIMD3<Float>, max: SIMD3<Float>) {
        guard var u = list.first else { return (.zero, .zero) }
        for b in list.dropFirst() { u = (simd_min(u.min, b.min), simd_max(u.max, b.max)) }
        return u
    }

    /// Frame the part again (⌘0).
    public func frame() { renderer.camera.frame(size: size) }

    public func apply(_ view: CameraView) { renderer.camera.apply(view) }

    private func currentRotation() -> simd_float3x3 { rotationMatrix(degrees: rotationDegrees) }

    func refreshPose() {
        guard !renderer.meshes.isEmpty else { return }
        let rot = currentRotation()
        renderer.setPose(rotation: rot, offset: seatingOffset(bounds: groupBounds, rotation: rot))
        refreshFlags()
    }

    /// The overhang colours (and, for a single part, the engine's seating offset) follow the pose live: the pose
    /// turns at once on the bounds-seated stand-in, and the analyses swap in as they land; the newest request
    /// wins, a stale answer is dropped.
    func refreshFlags() {
        let meshes = renderer.meshes
        guard !meshes.isEmpty else { return }
        generation += 1
        let mine = generation
        let rot = currentRotation(), threshold = thresholdDegrees
        for (i, mesh) in meshes.enumerated() {
            Task { @MainActor in
                let a = await mesh.analysis(thresholdDegrees: threshold, rotation: rot)
                guard mine == generation else { return }
                if meshes.count == 1, let o = a.offset { renderer.setPose(rotation: rot, offset: o) }
                renderer.setFlags(a.flags, part: i)
            }
        }
    }

    // MARK: input (points; the view passes its height for the gesture maths)

    public func orbit(dx: CGFloat, dy: CGFloat, viewHeight: CGFloat) {
        renderer.camera.orbit(dx: Float(dx), dy: Float(dy), viewHeight: Float(viewHeight))
    }

    public func pan(dx: CGFloat, dy: CGFloat, viewHeight: CGFloat) {
        renderer.camera.pan(dx: Float(dx), dy: Float(dy), viewHeight: Float(viewHeight))
    }

    public func zoom(magnification: CGFloat) { renderer.camera.zoom(by: Float(1 + magnification)) }
    public func zoom(wheelDelta: CGFloat) { renderer.camera.zoom(wheelDelta: Float(wheelDelta)) }

    /// Hover at a pixel of the drawable (origin top left); picks coalesce so only the newest runs behind one in flight.
    public func hover(x: Int, y: Int, width: Int, height: Int) {
        pendingPick = (x, y, width, height)
        guard !pickInFlight else { return }
        pickInFlight = true
        Task { @MainActor in
            while let p = pendingPick {
                pendingPick = nil
                let face = await renderer.pickFace(x: p.x, y: p.y, width: p.w, height: p.h)
                hoverFace = face
                renderer.setHover(face)
            }
            pickInFlight = false
        }
    }

    public func clearHover() {
        pendingPick = nil
        hoverFace = nil
        renderer.setHover(nil)
    }

    // MARK: benchmark

    /// Turn the view round the part at a steady rate and draw every display frame (the frame-time check).
    public var benchmarking: Bool { benchmarkLink != nil }

    public func setBenchmark(_ on: Bool) {
        benchmarkLink?.cancel()
        benchmarkLink = nil
        renderer.continuous = on
        guard on else { return }
        renderer.stats.reset()
        benchmarkLink = Task { @MainActor [weak self] in
            var last = ContinuousClock.now
            while !Task.isCancelled, let self {
                try? await Task.sleep(for: .milliseconds(4))
                let now = ContinuousClock.now
                let dt = Float((now - last) / .seconds(1))
                last = now
                renderer.camera.azimuth += 0.8 * dt          // 46 degrees a second, a slow steady turn
            }
        }
    }
}
