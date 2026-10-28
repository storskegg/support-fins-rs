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
    /// The pose: takes model coordinates to print coordinates (the session's `Pose`).
    public var rotation = matrix_identity_float3x3 { didSet { if rotation != oldValue { refreshPose() } } }
    public var volume = PrinterVolume.standard { didSet { renderer.setVolume(volume) } }
    public var highlightSmall = true { didSet { renderer.highlightSmall = highlightSmall } }
    /// The load of the Strength view, as a unit direction in the part's own frame (it turns with the part), or nil.
    public var loadDirection: SIMD3<Float>? { didSet { if loadDirection != oldValue { refreshHUD() } } }
    /// Show the layer frames around the part (View > Show layers).
    /// The midair starts of the current pose and whether Show midair starts is on (`web/ui/islands.js`). The cones
    /// are hidden while a ring turns the part: the starts are found after a drag, not during it.
    public var islandMarkers: [IslandMarker] = [] { didSet { if islandMarkers != oldValue { refreshIslands() } } }
    public var showsIslands = false { didSet { if showsIslands != oldValue { refreshIslands() } } }

    public var showsLayers = false { didSet { if showsLayers != oldValue { refreshHUD() } } }
    /// The debug frame-time overlay (View > Show Frame Time, debug builds only).
    public var showsFrameTime = false
    /// The posed part's size: the engine's when a single part has been analysed, else the rotated bounds.
    public private(set) var posedSize = SIMD3<Float>.zero

    /// Remove-fins mode (`web/ui/remove.js removeActive`): the pointer picks fins, not the part's faces. Hover
    /// reports the fin buffer's triangle under the pointer, a click (a press that did not drag) reports it
    /// again, and Esc, a right click or Ctrl+click calls `onCancelArmed`.
    public var supportPicking = false {
        didSet {
            guard supportPicking != oldValue else { return }
            pendingPick = nil
            hoverFace = nil
            renderer.setHover(nil)
            hoveringSupport = false
            if !supportPicking { renderer.setFinHighlight([]) }
        }
    }
    /// The fin-buffer triangle under the pointer in remove mode.
    public private(set) var hoverSupportTriangle: Int?
    /// The pointer is over a fin that a click would pick (the view shows a pointing hand).
    public private(set) var hoveringSupport = false { didSet { onCursorChange?() } }
    /// "Lay a face flat" is armed (`web/ui/pose.js layPlacing`): the rotate rings step aside, the face under the
    /// pointer is lit (the hover colour) with a pointing hand, a click (a press that did not drag) reports its
    /// part and face through `onFaceClick`, and Esc, a right click or Ctrl+click call `onFaceCancel`.
    public var facePicking = false {
        didSet {
            guard facePicking != oldValue else { return }
            clearHover()
            ringHover = nil
            updateGizmo()
            onCursorChange?()
        }
    }
    @ObservationIgnored public var onFaceClick: (@MainActor (_ part: Int, _ face: Int) -> Void)?
    @ObservationIgnored public var onFaceCancel: (@MainActor () -> Void)?
    /// The pointer is over a face that a click would lay flat.
    public var hoveringFaceToLay: Bool { facePicking && hoverFace != nil }
    @ObservationIgnored public var onSupportHover: (@MainActor (Int?) -> Void)?
    @ObservationIgnored public var onSupportClick: (@MainActor (Int?) -> Void)?
    @ObservationIgnored public var onCancelArmed: (@MainActor () -> Void)?
    @ObservationIgnored var onCursorChange: (@MainActor () -> Void)?
    // MARK: the rotate rings (`web/ui/pose.js`)

    /// "Show rotate rings" (`sf.showRings`): the rings are drawn, and grab the pointer, only while it is on.
    public var showRings = true { didSet { updateGizmo() } }
    /// The rings fade back while a sway brace is aimed (`ringfade.js`); 1 = fully shown.
    public var ringFade: Float = 1 { didSet { updateGizmo() } }
    public private(set) var ringHover: GizmoHandle? { didSet { updateGizmo() } }
    public private(set) var ringDragging: GizmoHandle? { didSet { updateGizmo(); refreshIslands() } }
    /// The degrees turned so far by the ring drag in progress.
    public private(set) var ringDegrees: Double = 0
    @ObservationIgnored public var onRing: ((RingEvent) -> Void)?

    public enum RingEvent {
        case began(GizmoHandle)
        /// The turn since the drag began: a world rotation through the part's centre (about the ring's axis, the
        /// view axis for the outer ring, or the trackball's axis for the middle).
        case turned(delta: simd_double3x3, degrees: Double, handle: GizmoHandle)
        case ended(turned: Bool)
    }

    @ObservationIgnored private var drag: (state: RotateGizmo.Drag, rotation: simd_float3x3, offset: SIMD3<Float>, center: SIMD3<Float>)?

    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var pickInFlight = false
    @ObservationIgnored private var pendingPick: (x: Int, y: Int, w: Int, h: Int)?
    @ObservationIgnored private var benchmarkLink: Task<Void, Never>?

    public init?(device: MTLDevice? = MTLCreateSystemDefaultDevice()) {
        guard let r = ViewerRenderer(device: device) else { return nil }
        renderer = r
        r.setVolume(volume)
        r.onCameraChange = { [weak self] in MainActor.assumeIsolated { self?.cameraRevision &+= 1 } }
        // the section plane's handle takes the part's rings' place while it is up (`planeHandleShown`)
        r.section.onChange = { [weak self] in MainActor.assumeIsolated { self?.updateGizmo(); self?.clearHover() } }
        r.section.onCursorChange = { [weak self] in MainActor.assumeIsolated { self?.onCursorChange?() } }
    }

    /// The section view (`web/ui/section.js`).
    public var section: SectionTool { renderer.section }

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
    public func frame() { stopGlide(); renderer.camera.frame(size: size) }

    /// Bumps whenever the camera moves, so a SwiftUI view that reads it (the view cube) redraws with it.
    public private(set) var cameraRevision = 0
    /// The Mouse preset (`web/ui/navigation.js`): which button orbits, pans and dollies.
    public var navPreset = NavPreset.default
    /// The view's height in points, the unit of an orbit (a full turn per view height).
    public var viewHeight: CGFloat = 0

    // MARK: camera views (`web/ui/camviews.js`)

    @ObservationIgnored private var glideTask: Task<Void, Never>?

    /// Glide round to look back along `dir` at the target, keeping the zoom: 450 ms ease-in-out (`glideTo`). A
    /// second call takes over from the first, and any drag of the view stops it.
    public func glide(toward dir: SIMD3<Float>) {
        stopGlide()
        let from = renderer.camera.offset
        let start = ContinuousClock.now
        glideTask = Task { @MainActor [weak self] in
            while !Task.isCancelled, let self {
                let t = (ContinuousClock.now - start) / .seconds(1)
                renderer.camera.setOffset(CameraGlide.offset(from: from, toward: dir, after: t))
                if t >= CameraGlide.seconds { break }
                try? await Task.sleep(for: .milliseconds(4))
            }
        }
    }

    public func stopGlide() { glideTask?.cancel(); glideTask = nil }

    /// Front, Back, Left, Right, Top or Bottom: the View popover's buttons and the View menu.
    public func apply(_ view: CameraView) { glide(toward: view.direction) }

    /// Home (`HOME_DIR`, the view cube's house): the starting angle, keeping the target and the zoom.
    public func goHome() { glide(toward: OrbitCamera.homeDirection) }

    private func currentRotation() -> simd_float3x3 { rotation }

    func refreshPose() {
        guard !renderer.meshes.isEmpty else { return }
        let rot = currentRotation()
        if let d = drag {
            // a ring drag turns the part about the centre it grabbed, not the seating point: the part must not
            // jump under the pointer. It is re-seated when the drag ends (the web's `shade()` note).
            let delta = rot * d.rotation.transpose
            renderer.setPose(rotation: rot, offset: delta * (d.offset - d.center) + d.center)
        } else {
            renderer.setPose(rotation: rot, offset: seatingOffset(bounds: groupBounds, rotation: rot))
        }
        updateGizmo()
        posedSize = Self.rotatedSize(groupBounds, rot)
        refreshHUD()
        refreshFlags()
    }

    static func rotatedSize(_ b: (min: SIMD3<Float>, max: SIMD3<Float>), _ rot: simd_float3x3) -> SIMD3<Float> {
        var lo = SIMD3<Float>(repeating: .infinity), hi = SIMD3<Float>(repeating: -.infinity)
        for i in 0..<8 {
            let c = SIMD3<Float>(i & 1 == 0 ? b.min.x : b.max.x, i & 2 == 0 ? b.min.y : b.max.y, i & 4 == 0 ? b.min.z : b.max.z)
            lo = simd_min(lo, rot * c)
            hi = simd_max(hi, rot * c)
        }
        return hi - lo
    }

    /// The load arrow through the middle of the part and the layer frames around the posed part
    /// (`ui/strength.js` `updateLoadArrowMesh`, `updateLayerView`).
    func refreshIslands() {
        let on = showsIslands && ringDragging == nil && !renderer.meshes.isEmpty
        renderer.setIslandMarkers(on ? islandMarkers : [], extent: max(posedSize.x, posedSize.y, posedSize.z))
    }

    func refreshHUD() {
        refreshIslands()
        guard !renderer.meshes.isEmpty else {
            renderer.clearLoadArrow()
            renderer.clearLayerFrames()
            return
        }
        if let d = loadDirection, simd_length(d) > 1e-6 {
            let center = (groupBounds.min + groupBounds.max) / 2
            renderer.setLoadArrow(center: center, direction: d, extent: max(size.x, size.y, size.z))
        } else {
            renderer.clearLoadArrow()
        }
        if showsLayers { renderer.setLayerFrames(size: posedSize) } else { renderer.clearLayerFrames() }
    }

    /// The overhang colours (and, for a single part, the engine's seating offset) follow the pose live. The pose
    /// turns at once on the bounds-seated stand-in; analyses run one at a time off the main actor and the newest
    /// pose wins, so a fast drag costs one analysis per finished one, not one per event.
    func refreshFlags() {
        guard !renderer.meshes.isEmpty else { return }
        flagsWanted = true
        guard !flagsRunning else { return }
        runFlags()
    }

    @ObservationIgnored private var flagsWanted = false
    @ObservationIgnored private var flagsRunning = false

    private func runFlags() {
        let meshes = renderer.meshes
        flagsWanted = false
        flagsRunning = true
        generation += 1
        let mine = generation
        let rot = currentRotation(), threshold = thresholdDegrees
        let dragging = drag != nil
        Task { @MainActor in
            let tasks = meshes.map { mesh in
                Task { @MainActor in await mesh.analysis(thresholdDegrees: threshold, rotation: rot, wantsSummary: !dragging) }
            }
            var results: [(Int, MeshAnalysis)] = []
            for (i, t) in tasks.enumerated() { results.append((i, await t.value)) }
            if mine == generation && meshes.count == renderer.meshes.count {
                for (i, a) in results {
                    if meshes.count == 1, !dragging, let o = a.offset { renderer.setPose(rotation: rot, offset: o) }
                    if meshes.count == 1, !dragging, let sz = a.size { posedSize = sz; refreshHUD() }
                    renderer.setFlags(a.flags, part: i)
                }
            }
            flagsRunning = false
            if flagsWanted { runFlags() }
        }
    }

    // MARK: ring input (pixels of the drawable, origin top left)

    func ndcRay(x: Int, y: Int, width: Int, height: Int) -> (origin: SIMD3<Float>, direction: SIMD3<Float>) {
        renderer.camera.ray(ndcX: 2 * Float(x) / Float(width) - 1, ndcY: 1 - 2 * Float(y) / Float(height))
    }

    /// The print-space middle of the part: its box centre through the pose.
    public var gizmoCenter: SIMD3<Float> {
        let c = (groupBounds.min + groupBounds.max) / 2
        return renderer.rotation * c + renderer.offset
    }

    func updateGizmo() {
        guard showRings, !supportPicking, !facePicking, !renderer.section.isOn, !renderer.meshes.isEmpty else { renderer.setGizmo(nil); return }
        renderer.setGizmo(GizmoDisplay(center: drag?.center ?? gizmoCenter, hover: ringHover, active: ringDragging, fade: ringFade))
    }

    /// The ring under a pixel, if the rings are on.
    public func ringAt(x: Int, y: Int, width: Int, height: Int) -> GizmoHandle? {
        guard showRings, !supportPicking, !facePicking, !renderer.section.isOn, !renderer.meshes.isEmpty, width > 0, height > 0, drag == nil else { return nil }
        return RotateGizmo(center: gizmoCenter).hit(atPixel: SIMD2(Float(x), Float(y)), camera: renderer.camera,
                                                    width: Float(width), height: Float(height))
    }

    /// A press: starts a ring drag when it lands on a ring. True when the ring took the press (no orbit then).
    public func beginRingDrag(x: Int, y: Int, width: Int, height: Int) -> Bool {
        guard let handle = ringAt(x: x, y: y, width: width, height: height) else { return false }
        let center = gizmoCenter
        guard let d = RotateGizmo(center: center).beginDrag(handle, camera: renderer.camera,
                                                            ray: ndcRay(x: x, y: y, width: width, height: height)) else { return false }
        drag = (d, rotation, renderer.offset, center)
        ringDragging = handle
        ringDegrees = 0
        clearHover()
        onRing?(.began(handle))
        return true
    }

    public func dragRing(x: Int, y: Int, width: Int, height: Int, snap: Bool) {
        guard let d = drag else { return }
        let gizmo = RotateGizmo(center: d.center)
        guard let t = gizmo.turn(d.state, ray: ndcRay(x: x, y: y, width: width, height: height), snap: snap) else { return }
        let degrees = t.radians * 180 / .pi
        guard degrees != ringDegrees else { return }
        ringDegrees = degrees
        let delta = RotateGizmo.rotation(axis: t.axis, radians: t.radians)
        rotation = simd_float3x3(columns: (SIMD3<Float>(delta.columns.0), SIMD3<Float>(delta.columns.1), SIMD3<Float>(delta.columns.2))) * d.rotation
        onRing?(.turned(delta: delta, degrees: degrees, handle: d.state.handle))
    }

    public func endRingDrag() {
        guard drag != nil else { return }
        let turned = ringDegrees != 0
        drag = nil
        ringDragging = nil
        onRing?(.ended(turned: turned))
        refreshPose()      // re-seat on the plate now the pivot may move again
    }

    public func clearRingHover() { if ringHover != nil { ringHover = nil } }

    /// Pointer moved over the view: light the ring under it, otherwise pick the face.
    public func pointerMoved(x: Int, y: Int, width: Int, height: Int) {
        // the section plane's handle owns the pointer: no pick lights up through it (`planeBusy`)
        section.pointerMoved(x: x, y: y, width: width, height: height)
        if section.busy { clearHover(); return }
        let ring = ringAt(x: x, y: y, width: width, height: height)
        if ring != ringHover { ringHover = ring }
        if ring != nil { clearHover() } else { hover(x: x, y: y, width: width, height: height) }
    }

    // MARK: input (points; the view passes its height for the gesture maths)

    public func orbit(dx: CGFloat, dy: CGFloat, viewHeight: CGFloat) {
        stopGlide()
        renderer.camera.orbit(dx: Float(dx), dy: Float(dy), viewHeight: Float(viewHeight))
    }

    public func pan(dx: CGFloat, dy: CGFloat, viewHeight: CGFloat) {
        stopGlide()
        renderer.camera.pan(dx: Float(dx), dy: Float(dy), viewHeight: Float(viewHeight))
    }

    public func zoom(magnification: CGFloat) { stopGlide(); renderer.camera.zoom(by: Float(1 + magnification)) }
    public func zoom(wheelDelta: CGFloat) { stopGlide(); renderer.camera.zoom(wheelDelta: Float(wheelDelta)) }

    /// Perform a preset's action for a drag of (`dx`, `dy`) points; a dolly zooms by the drag's height, as
    /// OrbitControls' `handleMouseMoveDolly` does.
    public func navigate(_ action: NavAction, dx: CGFloat, dy: CGFloat, viewHeight: CGFloat) {
        switch action {
        case .orbit: orbit(dx: dx, dy: dy, viewHeight: viewHeight)
        case .pan: pan(dx: dx, dy: dy, viewHeight: viewHeight)
        case .dolly: zoom(wheelDelta: dy)
        case .none: break
        }
    }

    /// Hover at a pixel of the drawable (origin top left); picks coalesce so only the newest runs behind one in flight.
    public func hover(x: Int, y: Int, width: Int, height: Int) {
        pendingPick = (x, y, width, height)
        guard !pickInFlight else { return }
        pickInFlight = true
        Task { @MainActor in
            while let p = pendingPick {
                pendingPick = nil
                if supportPicking {
                    let tri = await renderer.pickSupport(x: p.x, y: p.y, width: p.w, height: p.h)
                    guard supportPicking else { continue }
                    hoverSupportTriangle = tri
                    if section.isOn {
                        let face = await renderer.pickFace(x: p.x, y: p.y, width: p.w, height: p.h)
                        section.overPart = tri != nil || face != nil
                    }
                    hoveringSupport = tri != nil
                    onSupportHover?(tri)
                    continue
                }
                let face = await renderer.pickFace(x: p.x, y: p.y, width: p.w, height: p.h)
                let before = hoveringFaceToLay
                section.overPart = face != nil
                hoverFace = face
                // the web lights the hovered face only while Lay a face flat is armed (`web/ui/pose.js layHover`)
                renderer.setHover(facePicking ? face : nil)
                if hoveringFaceToLay != before { onCursorChange?() }
            }
            pickInFlight = false
        }
    }

    public func clearHover() {
        pendingPick = nil
        section.overPart = false
        let wasLit = hoveringFaceToLay
        hoverFace = nil
        renderer.setHover(nil)
        if wasLit { onCursorChange?() }
        if supportPicking, hoverSupportTriangle != nil {
            hoverSupportTriangle = nil
            hoveringSupport = false
            onSupportHover?(nil)
        }
    }

    /// A click at a pixel of the drawable (origin top left): in lay-a-face mode the part and face under it; in remove mode the fin-buffer triangle
    /// under it (nil: empty space). Does nothing when the mode is off.
    public func click(x: Int, y: Int, width: Int, height: Int) {
        if facePicking {
            Task { @MainActor in
                guard let face = await renderer.pickFace(x: x, y: y, width: width, height: height), facePicking,
                      let at = renderer.locate(face) else { return }
                onFaceClick?(at.part, at.face)
            }
            return
        }
        guard supportPicking else { return }
        Task { @MainActor in
            let tri = await renderer.pickSupport(x: x, y: y, width: width, height: height)
            guard supportPicking else { return }
            onSupportClick?(tri)
        }
    }

    /// Esc, a right click or Ctrl+click while a pointer mode is armed (`app.js cancelArmed`). True when it was
    /// armed, i.e. the event was the mode's.
    @discardableResult
    public func cancelArmed() -> Bool {
        if facePicking { onFaceCancel?(); return true }
        guard supportPicking else { return false }
        onCancelArmed?()
        return true
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
