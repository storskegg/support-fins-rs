import Foundation
import Metal
import Observation
import SupportFinsCore
import Viewer
import simd

/// Hand-placed supports (`web/ui/walls.js`): the breakaway walls laid by clicking two points (a curve: two ends and a
/// point it runs through) in Draw mode or with "+ Add walls by hand", and the sway braces stood with one click on an
/// upright side. This is the interaction and the state; the geometry is the engine's (`DrawKit`, docs/FFI.md "M6
/// additions: Draw"), the drawing is the viewer's (`DrawScene`).
///
/// The walls live in the session (`PartSession.drawnWalls`), in the part's own frame, so a placement is an undo step and
/// a wall follows the part through later turns. Each pose, settings change, wall change or new build re-stands them
/// (`sweepDrawn`); the clicks are the web's, in the web's order.
@MainActor
@Observable
final class DrawModel {
    enum Shape: String, Sendable { case line, curve }

    unowned let host: ViewerModel

    /// Line or Curve (`support_fins_draw_shape`, remembered).
    private(set) var shape: Shape
    /// A wall is half drawn: Back one point is offered.
    private(set) var started = false
    /// The selected support (`selectedWall`).
    private(set) var selectedID: UUID?
    /// `drawMsg`: the last placement result, for the readout.
    private(set) var message = ""
    /// What the last sweep made of each wall, by wall id.
    private(set) var outcomes: [UUID: WallOutcome] = [:]
    /// The vertices of the hand-placed supports' last sweep, for the fit and the grams (`activeAdded`).
    private(set) var added: AddedGeometry?
    /// Counts the sweeps that landed, so what reads the sweep (the export, the readout) is told.
    private(set) var sweepRevision = 0
    /// The brace preview's label and its motion.
    let motion = BraceMotion()

    // MARK: internals

    @ObservationIgnored private var start: SIMD3<Double>?          // local
    @ObservationIgnored private var end: SIMD3<Double>?            // local, a curve's other end
    @ObservationIgnored private var cursor: SIMD3<Double>?         // world
    @ObservationIgnored private var cursorVisible = false
    @ObservationIgnored private var ghostMessage = ""
    @ObservationIgnored private var ghostMesh: OverlayMesh?
    @ObservationIgnored private var bandPoints: [SIMD3<Double>] = []
    @ObservationIgnored private var wallsMesh: OverlayMesh?
    @ObservationIgnored private var selectionMesh: OverlayMesh?
    @ObservationIgnored fileprivate(set) var sweep: DrawnSweep?
    @ObservationIgnored private(set) var sweptIDs: [UUID] = []
    @ObservationIgnored private var kit: DrawKit?
    @ObservationIgnored private var kitKey: KitKey?
    /// What the session wants a kit for now (`wantedKit`), set by `sync`.
    @ObservationIgnored private var wanted: KitKey?
    @ObservationIgnored private var kitRot = matrix_identity_double3x3
    @ObservationIgnored private var kitOffset = SIMD3<Double>.zero
    @ObservationIgnored private var kitTask: Task<Void, Never>?
    @ObservationIgnored private var sweepGeneration = 0
    @ObservationIgnored private(set) var sweptKey: SweepKey?
    /// The key of the sweep that has landed (`sweptKey` is the newest one asked for).
    @ObservationIgnored private(set) var landedKey: SweepKey?
    @ObservationIgnored private var ghostRequest: (a: SIMD3<Double>, b: SIMD3<Double>, m: SIMD3<Double>?)?
    @ObservationIgnored private var ghostTask: Task<Void, Never>?
    @ObservationIgnored private var braceRequest: (face: Int, point: SIMD3<Double>)?
    @ObservationIgnored private var braceTask: Task<Void, Never>?
    /// The brace key the ghost and tip show now (`braceShown`).
    @ObservationIgnored private var braceShown: String?
    @ObservationIgnored private var viewHeight = 1.0
    @ObservationIgnored private var lastHandMode = ViewerController.HandPicking.off

    static let shapeKey = "support_fins_draw_shape"
    /// `SNAP_PX`: curve ends snap to another wall's end within this many CSS pixels.
    static let snapPixels = 10.0

    init(host: ViewerModel) {
        self.host = host
        shape = UserDefaults.standard.string(forKey: Self.shapeKey) == "curve" ? .curve : .line
        motion.onChange = { [weak self] in self?.refresh() }
        motion.onRings = { [weak self] fade in self?.controller?.ringFade = fade }
        if let c = host.controller {
            c.onHandMove = { [weak self] p in self?.pointerMoved(p) }
            c.onHandClick = { [weak self] p in self?.pointerClicked(p) }
            c.onHandCancel = { [weak self] escape in self?.cancel(escape: escape) ?? false }
            c.onHandKey = { [weak self] key in self?.key(key) ?? false }
            c.canHandDelete = { [weak self] in self?.canDelete ?? false }
        }
    }

    var session: PartSession { host.session }
    var controller: ViewerController? { host.controller }
    private var build: BuildModel? { host.wiring?.build }

    // MARK: what is on

    /// `drawActive`: the pointer lays walls (Draw, or "+ Add walls by hand").
    var drawActive: Bool { session.finsOn && (session.settings.mode == .draw || session.addByHand) }
    /// `braceActive`: Auto / Full with Sway braces on and no "+ Add" in progress.
    var braceActive: Bool {
        session.finsOn && session.settings.mode != .draw && !session.addByHand && session.settings.sway
            && controller?.facePicking != true && host.wiring?.removal?.armed != true
    }
    /// `drawShown`: the hand-placed supports are on screen and in the export.
    var drawShown: Bool {
        session.finsOn && (session.settings.mode == .draw
            || (session.settings.mode != .draw && (session.addByHand || !session.drawnWalls.isEmpty)))
    }
    var autoLike: Bool { session.settings.mode != .draw }
    /// The "+ Add walls by hand" button is offered (`syncAugmentUI`).
    var augmentOffered: Bool { session.finsOn && autoLike }
    /// The controls (`syncDrawControls`): the box with Clear is shown whenever hand-placed supports are.
    var controlsShown: Bool { drawShown }
    var hintShown: Bool { drawActive }
    var hasWalls: Bool { !session.drawnWalls.isEmpty }
    var selected: DrawnWall? { selectedID.flatMap { id in session.drawnWalls.first { $0.id == id } } }
    /// The engine's kit for the pose is ready (the pointer can be read).
    var kitReady: Bool { kitIsCurrent }
    /// The last sweep is of the walls, the pose and the build on show.
    var sweepCurrent: Bool { sweep != nil && landedKey != nil && landedKey == currentSweepKey() }
    /// Backspace / Delete would do something (the Edit > Delete item).
    var canDelete: Bool { (drawActive && started) || selected != nil }

    /// The hint under the Line / Curve control (`syncDrawControls`).
    var hint: String {
        shape == .curve
            ? "Click the two ends of a curved wall on the red faces, then a point it runs through — out past the ends for a bigger arc. Ends snap to another wall’s end, so two curves close a ring. Backspace takes back a point; Esc or right-click cancels."
            : "Click two points across an overhang — straight onto the red faces — to lay a breakaway wall along that line. Hover an upright side to preview a sway brace: its top follows the cursor, and near the top of the side it runs full height. Click to stand it. Esc or right-click cancels."
    }

    /// The "+ Add walls by hand" button's text (`syncAugmentUI`).
    var augmentTitle: String { session.addByHand ? "Done adding walls" : "+ Add walls by hand" }

    func setShape(_ new: Shape) {
        guard new != shape else { return }
        shape = new
        UserDefaults.standard.set(new.rawValue, forKey: Self.shapeKey)
        clearPreview()           // a wall half drawn as the other shape starts over
        refresh()
    }

    /// The augment toggle (`augment-toggle`): no undo step, as the web.
    func toggleAugment() {
        if host.wiring?.removal?.armed == true { host.wiring?.removal?.disarm(); host.wiring?.show() }
        session.addByHand.toggle()
        message = ""
        clearPreview()
        sync()
        // the web re-runs the build (`settings.js` augment-toggle handler calls `refreshFins()`)
        if session.finsOn { host.wiring?.scheduler?.refresh() }
    }

    // MARK: the window's changes (WindowModifier.onChange)

    /// Everything a change of which re-stands the supports or changes the mode of the pointer.
    struct SyncKey: Equatable {
        var pose: Pose
        var threshold: Double
        var settings: String
        var tunables: String
        var parts: [ObjectIdentifier]
        var walls: [DrawnWall]
        var finsOn: Bool
        var mode: FinSettings.Mode
        var addByHand: Bool
        var swayOn: Bool
        var build: ObjectIdentifier?
        var building: Bool
        var removed: Set<FinKey>
        var facePicking: Bool
        var removalArmed: Bool
        var dragging: Bool
    }

    var syncKey: SyncKey {
        SyncKey(pose: session.pose, threshold: session.settings.threshold, settings: session.settings.drawSettingsJSON(),
                tunables: session.settings.tunablesJSON(), parts: session.parts.map { ObjectIdentifier($0) },
                walls: session.drawnWalls, finsOn: session.finsOn, mode: session.settings.mode, addByHand: session.addByHand,
                swayOn: session.settings.sway, build: build?.latest.map { ObjectIdentifier($0) }, building: build?.building ?? false,
                removed: session.removedFins, facePicking: controller?.facePicking ?? false,
                removalArmed: host.wiring?.removal?.armed ?? false, dragging: session.ringDragging)
    }

    /// The window's state changed: which pointer mode is on, the kit for the pose, the sweep.
    func sync() {
        // Starting Remove fins ends "+ Add" (`remove.js:123`); fins off do too (`settings.js:416`).
        if session.addByHand, host.wiring?.removal?.armed == true || !session.finsOn { session.addByHand = false }
        if session.settings.mode == .draw, session.addByHand { session.addByHand = false }
        let mode: ViewerController.HandPicking = drawActive ? .walls : (braceActive ? .braces : .off)
        if mode != lastHandMode {
            lastHandMode = mode
            clearPreview()
            if mode == .off { message = "" }
        }
        controller?.handPicking = mode
        if mode == .off { motion.drop() }
        host.wiring?.exporter?.drawn = { [weak self] in self?.exportDrawn ?? ExportModel.Drawn(sweep: nil, pending: false) }
        if let board = host.features[PlatesFeature.self]?.board, let draw = host.features[DrawFeature.self] {
            board.drawn = { [weak draw, unowned board] m in draw?.exportDrawn(m, board: board) }
            board.walls = { [weak draw, unowned board] m in draw?.wallsOf(m, board: board) ?? [] }
        }
        ensureKit()
        resweep()
        refresh()
    }

    /// A new part: the walls were the old part's.
    func partChanged() {
        clearPreview()
        message = ""
        selectedID = nil
        kit = nil; kitKey = nil; kitTask?.cancel()
        sweep = nil; sweptKey = nil; landedKey = nil; sweptIDs = []; outcomes = [:]; added = nil
        wallsMesh = nil; selectionMesh = nil
        refresh()
    }

    // MARK: the kit

    private struct KitKey: Equatable {
        var part: ObjectIdentifier
        var rot: [Double]
        var threshold: Double
        var settings: String
        var tunables: String
    }

    private var wantedKit: KitKey? {
        guard let part = session.parts.first, drawShown || drawActive || braceActive else { return nil }
        return KitKey(part: ObjectIdentifier(part), rot: session.pose.columnMajor, threshold: session.settings.threshold,
                      settings: session.settings.drawSettingsJSON(), tunables: session.settings.tunablesJSON())
    }

    /// The pose the supports are stood for is the session's (not lagging behind a turn in progress).
    private var kitIsCurrent: Bool { kit != nil && wanted != nil && kitKey == wanted }

    private func ensureKit() {
        wanted = wantedKit
        guard let want = wanted, let core = session.parts.first else {
            kit = nil; kitKey = nil; kitTask?.cancel()
            return
        }
        // a ring is turning the part: the supports wait for the release, drawn pale meanwhile
        if session.ringDragging { return }
        guard kitKey != want else { return }
        kit = nil
        kitKey = want
        kitTask?.cancel()
        let part = core.part, rot = want.rot
        kitTask = Task { @MainActor [weak self] in
            let made = await Task.detached(priority: .userInitiated) {
                try? DrawKit(part: part, rot: rot, thresholdDeg: want.threshold, settingsJson: want.settings, tunablesJson: want.tunables)
            }.value
            guard let self, !Task.isCancelled, self.kitKey == want, let made else { return }
            self.kit = made
            let o = made.offset()
            self.kitOffset = o.count == 3 ? SIMD3(o[0], o[1], o[2]) : .zero
            self.kitRot = Pose(columnMajor: rot)?.rotation ?? matrix_identity_double3x3
            self.resweep()
            self.refresh()
        }
    }

    // MARK: frames

    func world(_ local: SIMD3<Double>) -> SIMD3<Double> { kitRot * local + kitOffset }
    func local(_ world: SIMD3<Double>) -> SIMD3<Double> { kitRot.transpose * (world - kitOffset) }

    // MARK: the sweep

    struct SweepKey: Equatable {
        var kit: ObjectIdentifier
        var walls: [DrawnWall]
        var build: ObjectIdentifier?
        var removed: Set<FinKey>
        var autoOn: Bool
    }

    /// What the current state would sweep, to compare with `sweptKey`.
    var currentKey: SweepKey? { currentSweepKey() }

    private func currentSweepKey() -> SweepKey? {
        guard let kit, kitIsCurrent else { return nil }
        return SweepKey(kit: ObjectIdentifier(kit), walls: session.drawnWalls, build: build?.latest.map { ObjectIdentifier($0) },
                        removed: session.removedFins, autoOn: session.finsOn && autoLike)
    }

    /// `rebuildDrawn`: stand the walls for the pose, clear of Auto's supports when they are on screen.
    func resweep() {
        guard drawShown else { dropSweep(); return }
        guard let kit, let key = currentSweepKey(), key != sweptKey else { return }
        sweptKey = key
        sweepGeneration += 1
        let generation = sweepGeneration
        let walls = session.drawnWalls
        let auto = key.autoOn ? build?.latest : nil
        let ids = key.autoOn ? (build?.matchedRemovals(of: session.removedFins).keys.sorted().map { UInt64($0) } ?? []) : []
        let device = controller?.renderer.device
        Task { @MainActor [weak self] in
            let made = await Task.detached(priority: .userInitiated) { () -> (DrawnSweep, [WallOutcome])? in
                guard let s = try? kit.sweep(wallsJson: walls.json, autoBuild: auto, removed: ids) else { return nil }
                return (s, WallOutcome.decode(s.wallsJson()))
            }.value
            guard let self, generation == self.sweepGeneration, let (s, outs) = made else { return }
            self.land(s, outs, walls: walls, key: key, device: device)
        }
    }

    private func land(_ s: DrawnSweep, _ outs: [WallOutcome], walls: [DrawnWall], key: SweepKey, device: MTLDevice?) {
        sweep = s
        landedKey = key
        sweptIDs = walls.map(\.id)
        outcomes = Dictionary(uniqueKeysWithValues: zip(walls.map(\.id), outs))
        let tris = s.triangles()
        wallsMesh = tris.count() > 0 ? device.flatMap { d in tris.metalBuffer(device: d) }.map { OverlayMesh(buffer: $0, vertices: Int(tris.count())) } : nil
        added = AddedGeometry(json: s.addedJson())
        braceShown = nil
        motion.drop()          // the braces a preview stood clear of have moved
        sweepRevision += 1
        syncSelection()
        refresh()
    }

    private func dropSweep() {
        guard sweep != nil || wallsMesh != nil else { return }
        sweep = nil; sweptKey = nil; landedKey = nil; sweptIDs = []; outcomes = [:]; added = nil; wallsMesh = nil; selectionMesh = nil
        sweepRevision += 1
        refresh()
    }

    /// `syncSelection`: re-draw the highlight for the current selection, or clear a stale one.
    private func syncSelection() {
        selectionMesh = nil
        if let id = selectedID {
            guard drawShown, let i = session.drawnWalls.firstIndex(where: { $0.id == id }), outcomes[id]?.ok == true,
                  let sweep, sweptIDs == session.drawnWalls.map(\.id) else {
                if !(drawShown && session.drawnWalls.contains { $0.id == id }) { selectedID = nil }
                return
            }
            let tris = sweep.wallTriangles(index: UInt64(i))
            if tris.count() > 0, let device = controller?.renderer.device, let b = tris.metalBuffer(device: device) {
                selectionMesh = OverlayMesh(buffer: b, vertices: Int(tris.count()))
            }
        }
    }

    // MARK: the scene

    /// Put everything on the viewer (`sizeMarkers` is the renderer's).
    func refresh() {
        guard let controller else { return }
        var scene = DrawScene()
        scene.walls = drawShown ? wallsMesh : nil
        scene.selection = drawShown ? selectionMesh : nil
        scene.ghost = ghostMesh
        scene.braces = motion.ghosts.map { (mesh: $0.mesh, opacity: $0.opacity) }
        scene.band = bandPoints.map { SIMD3<Float>($0) }
        scene.bandBad = !ghostMessage.isEmpty
        scene.dim = controller.renderer.dimSupports || (drawShown && !kitIsCurrent)
        var markers: [OverlayMarker] = []
        if let s = start, started {
            markers.append(OverlayMarker(position: SIMD3<Float>(world(s)), radiusPixels: DrawPalette.dotPixels, color: DrawPalette.dot, alpha: 1))
        }
        if let e = end {
            markers.append(OverlayMarker(position: SIMD3<Float>(world(e)), radiusPixels: DrawPalette.dotPixels, color: DrawPalette.dot, alpha: 1))
        }
        if cursorVisible, let c = cursor {
            markers.append(OverlayMarker(position: SIMD3<Float>(c), radiusPixels: DrawPalette.cursorPixels, color: motion.cursorColor,
                                         alpha: DrawPalette.cursorOpacity))
        }
        scene.markers = markers
        controller.renderer.setDrawScene(scene.isEmpty ? nil : scene)
        controller.handCursor = cursorVisible
    }

    // MARK: the pointer

    private struct Surface {
        var point: SIMD3<Double>
        var face: Int
        var distance: Double
    }

    /// The spot on the part under the pointer (`pickFace`'s hit): the id pass found the face, the ray says where.
    private func surface(_ p: ViewerController.HandPointer) -> Surface? {
        guard let kit, kitIsCurrent, let f = p.face, f.part == 0,
              let h = try? kit.surfaceHit(face: UInt64(f.face), origin: [p.origin.x, p.origin.y, p.origin.z],
                                          dir: [p.direction.x, p.direction.y, p.direction.z]),
              h.point.count == 3 else { return nil }
        return Surface(point: SIMD3(h.point[0], h.point[1], h.point[2]), face: f.face, distance: h.distance)
    }

    /// `swayHit`: a click on an UPRIGHT side (no wall half-drawn) stands a brace.
    private func swayHit(_ s: Surface) -> Bool {
        start == nil && session.settings.sway && (kit?.faceTakesBrace(face: UInt64(s.face)) ?? false)
    }

    /// Pointer move: the cursor, band and ghost wall track the surface under it (`drawHover`, `braceHover`).
    func pointerMoved(_ p: ViewerController.HandPointer?) {
        guard let controller else { return }
        guard let p else { pointerLeft(); return }
        viewHeight = Double(p.size.height)
        let hit = surface(p)
        switch controller.handPicking {
        case .walls:
            if let hit, swayHit(hit) {
                updateBracePreview(hit, p)
            } else if let hit {
                _ = slipOff(hit, keepCursor: true)       // Draw's own cursor carries on over the rest of the part
                updatePreview(hit.point)
            } else if start == nil {
                bandPoints = []
                braceAway()                               // off the part: a gap crossed on the way doesn't blink the brace
            } else {
                braceQueued(nil)
                cursorVisible = false; bandPoints = []
                braceShown = nil
                motion.hide(soft: false)
                ghostMesh = nil
            }
        case .braces:
            if let hit, swayHit(hit) {
                updateBracePreview(hit, p)
            } else if let hit, slipOff(hit, keepCursor: false) || motion.graceActive {
                cursor = hit.point                        // still fading in its grace: keep following
            } else {
                braceAway()
            }
        case .off:
            break
        }
        refresh()
    }

    /// The pointer left the view, or an orbit took it: the preview goes (`dropBracePreview`).
    private func pointerLeft() {
        braceQueued(nil)
        if start == nil || controller?.handPicking == .braces {
            cursorVisible = false
            braceShown = nil
            motion.hide(soft: false)
        }
        refresh()
    }

    private func braceQueued(_ r: (face: Int, point: SIMD3<Double>)?) { braceRequest = r }

    /// `braceAway`: the pointer left the part: the same grace as `slipOff`, then the preview and its dot go.
    private func braceAway() {
        braceQueued(nil)
        if braceShown == nil && !motion.graceActive {
            cursorVisible = false            // the dot too, even with the preview already gone
            return
        }
        braceShown = nil
        motion.hide(soft: true) { [weak self] in self?.cursorVisible = false; self?.refresh() }
    }

    /// The pointer is on the part but off an upright side: the dot follows it, and the brace stays a moment (an edge or
    /// a small facet on the way) before it fades. True when a brace was showing.
    private func slipOff(_ hit: Surface, keepCursor: Bool) -> Bool {
        braceQueued(nil)
        // back on the part inside a grace begun off it: Draw's cursor stays after all
        if braceShown == nil && keepCursor && motion.graceActive { motion.hide(soft: true, then: nil) }
        guard braceShown != nil else { return false }
        braceShown = nil
        cursor = hit.point
        motion.hide(soft: true, then: keepCursor ? nil : { [weak self] in self?.cursorVisible = false; self?.refresh() })
        return true
    }

    // MARK: the wall in progress

    /// Ends snap to another drawn wall's end within `snapPixels` on screen, so curves chain into a ring. Curves only: a
    /// straight wall lands where it is clicked, as it always has.
    func snapEnd(_ p: SIMD3<Double>) -> SIMD3<Double> {
        guard shape == .curve, let camera = controller?.renderer.camera else { return p }
        let eye = SIMD3<Double>(camera.eye)
        let tol = Self.snapPixels * 2 * Double(tan(camera.fovY / 2)) * simd_distance(eye, p) / max(1, viewHeight)
        var best: SIMD3<Double>?
        var bd = tol
        for w in session.drawnWalls where w.kind != .sway {
            for e in [w.a, w.b] {
                let q = world(e)
                let d = simd_distance(q, p)
                if d < bd { bd = d; best = q }
            }
        }
        return best ?? p
    }

    /// Show the endpoint / cursor / band, and a live ghost of the wall in progress. A curve with both ends down bends
    /// through the cursor: its third click (`updatePreview`).
    private func updatePreview(_ hitPoint: SIMD3<Double>) {
        let p = end != nil ? hitPoint : snapEnd(hitPoint)
        cursor = p
        cursorVisible = true
        guard let s = start else { bandPoints = []; return }
        let a = world(s)
        let b = end.map(world)
        bandPoints = b.map { arcPoints(a, $0, p) } ?? [a, p]
        queueGhost(a: a, b: b ?? p, m: b == nil ? nil : p)
    }

    /// World points round the arc a -> b through m (a straight line when in line): `arcPoints`.
    private func arcPoints(_ a: SIMD3<Double>, _ b: SIMD3<Double>, _ m: SIMD3<Double>) -> [SIMD3<Double>] {
        guard let flat = try? arcBand(a: [a.x, a.y, a.z], b: [b.x, b.y, b.z], m: [m.x, m.y, m.z]) else { return [a, b] }
        return stride(from: 0, to: flat.count - 2, by: 3).map { SIMD3(flat[$0], flat[$0 + 1], flat[$0 + 2]) }
    }

    /// Build the ghost at most once per frame: one wall over the whole part is a few ms, fine occasionally but not at
    /// raw pointer-move rates (`ghostQueued`).
    private func queueGhost(a: SIMD3<Double>, b: SIMD3<Double>, m: SIMD3<Double>?) {
        ghostRequest = (a, b, m)
        guard ghostTask == nil, let kit else { return }
        ghostTask = Task { @MainActor [weak self] in
            while let self, let q = self.ghostRequest {
                self.ghostRequest = nil
                let r = await Task.detached(priority: .userInitiated) {
                    try? kit.ghost(a: [q.a.x, q.a.y, q.a.z], b: [q.b.x, q.b.y, q.b.z], m: q.m.map { [$0.x, $0.y, $0.z] })
                }.value
                guard !Task.isCancelled, self.start != nil, self.controller?.handPicking == .walls, let r else { continue }
                self.ghostMesh = r.ok() ? self.mesh(r.triangles()) : nil
                // a curve says at once why it can't bend that tight; a straight wall in progress keeps its quiet ghost
                // (its end is not chosen yet)
                self.showGhostMessage(q.m != nil && !r.ok() && r.reason().contains("too tight") ? r.reason() : "")
                self.refresh()
            }
            self?.ghostTask = nil
        }
    }

    /// A curve the ghost can't build says why in the readout, as soon as it can't (a placed wall's refusal already
    /// does): changed only when the reason does (`showGhostMsg`).
    private func showGhostMessage(_ msg: String) {
        guard msg != ghostMessage else { return }
        ghostMessage = msg
        message = msg.isEmpty ? "" : "can’t place this curve: \(msg)"
    }

    private func mesh(_ buffer: GpuBuffer) -> OverlayMesh? {
        guard buffer.count() > 0, let device = controller?.renderer.device, let b = buffer.metalBuffer(device: device) else { return nil }
        return OverlayMesh(buffer: b, vertices: Int(buffer.count()))
    }

    /// Drop any wall-in-progress and hide every transient draw visual (`clearPreview`).
    func clearPreview() {
        start = nil; end = nil
        started = false
        ghostRequest = nil
        cursorVisible = false
        bandPoints = []
        ghostMesh = nil
        showGhostMessage("")
        dropGhost()
        refresh()
    }

    /// `dropGhost`: no wall ghost, no brace.
    private func dropGhost() {
        ghostMesh = nil
        braceShown = nil
        braceRequest = nil
        motion.drop()
    }

    /// Backspace: take back the last point of the wall in progress (a curve's end, or its start -- which drops the wall).
    /// True when there was one (`undoDrawPoint`).
    @discardableResult
    func undoPoint() -> Bool {
        guard start != nil else { return false }
        if end != nil {
            end = nil
            ghostRequest = nil
            showGhostMessage("")
            ghostMesh = nil
            if cursorVisible, let c = cursor { updatePreview(c) } else { bandPoints = [] }
        } else {
            clearPreview()
        }
        refresh()
        return true
    }

    // MARK: the sway brace preview

    /// Hovering an upright side with Sway braces on: ghost + tip for the brace a click there stands
    /// (`updateBracePreview`). At most one brace build at a time, the newest request.
    private func updateBracePreview(_ hit: Surface, _ p: ViewerController.HandPointer) {
        cursor = hit.point
        cursorVisible = true
        bandPoints = []
        motion.moveTip(to: CGPoint(x: p.location.x + 16, y: p.location.y + 16))
        braceRequest = (hit.face, hit.point)
        guard braceTask == nil, let kit else { return }
        let sweepNow = sweep
        braceTask = Task { @MainActor [weak self] in
            while let self, let q = self.braceRequest {
                self.braceRequest = nil
                guard self.start == nil, self.controller?.handPicking != .off else { continue }
                let auto = self.session.finsOn && self.autoLike ? self.build?.latest : nil
                let removed = self.removedIDs()
                let b = await Task.detached(priority: .userInitiated) {
                    try? kit.braceAt(face: UInt64(q.face), point: [q.point.x, q.point.y, q.point.z], sweep: sweepNow, autoBuild: auto, removed: removed)
                }.value
                guard let b, self.controller?.handPicking != .off else { continue }
                if b.key() == self.braceShown { continue }
                self.braceShown = b.key()
                let mm = JS.round(b.height())
                let text = !b.ok() ? b.reason()
                    : b.full() ? JS.t("Brace · full height {mm} mm", ["mm": JS.number(mm)])
                    : JS.t("Brace · {mm} mm · move up for full height", ["mm": JS.number(mm)])
                self.motion.show(b.ok() ? self.mesh(b.triangles()) : nil, height: b.height(), text: text, bad: !b.ok())
                self.refresh()
            }
            self?.braceTask = nil
        }
    }

    private func removedIDs() -> [UInt64] {
        guard session.finsOn, autoLike else { return [] }
        return build?.matchedRemovals(of: session.removedFins).keys.sorted().map { UInt64($0) } ?? []
    }

    // MARK: clicks

    /// A click: select a placed support, stand a sway brace, or set / commit a wall end (`drawClick`, `braceClick`).
    func pointerClicked(_ p: ViewerController.HandPointer) {
        guard let controller else { return }
        viewHeight = Double(p.size.height)
        let hit = surface(p)
        // a click on a support you placed selects it (for Delete / Remove selected), unless a wall is half-drawn: then
        // the click is its second point
        if start == nil, let i = pickSupport(p, hit) { select(session.drawnWalls[i].id); return }
        guard let hit else { return }
        if controller.handPicking == .braces {
            guard swayHit(hit) else { return }
            if selectedID != nil { selectedID = nil; syncSelection() }
            placeBrace(hit)
            return
        }
        if selectedID != nil { selectedID = nil; syncSelection() }
        // with Sway braces on, a single click on an UPRIGHT side stands a brace there; a click on anything else still
        // starts a two-point wall as before
        if swayHit(hit) { placeBrace(hit); return }
        if start == nil {
            dropGhost()              // a brace still fading in its grace is not what this click starts
            start = local(snapEnd(hit.point))
            started = true
            message = ""
            updatePreview(hit.point)
        } else if shape == .curve && end == nil {
            end = local(snapEnd(hit.point))
            updatePreview(hit.point)
        } else if let e = end {
            placeWall(to: world(e), through: hit.point)
        } else {
            placeWall(to: hit.point, through: nil)
        }
        refresh()
    }

    /// The hand-placed support under the pointer, if it is nearer than the part (`pickSupport`): its index. With a
    /// section on, both hits skip the cut-away half (`keptHit`): the part's is the id pass's, which the cut clips; the
    /// walls' passes over hits behind the plane.
    private func pickSupport(_ p: ViewerController.HandPointer, _ hit: Surface?) -> Int? {
        let keep = sectionKeep
        guard let sweep, drawShown, sweptIDs == session.drawnWalls.map(\.id),
              let i = try? sweep.pick(origin: [p.origin.x, p.origin.y, p.origin.z], dir: [p.direction.x, p.direction.y, p.direction.z],
                                      partDistance: hit?.distance, keep: keep) else { return nil }
        return Int(i)
    }

    /// The section's plane (`[nx, ny, nz, w]`, kept where `n . p + w >= 0`, print space), nil with no section.
    var sectionKeep: [Double]? {
        guard let section = controller?.section, section.isOn else { return nil }
        let c = section.plane.clip
        return [Double(c.x), Double(c.y), Double(c.z), Double(c.w)]
    }

    /// Commit the wall from `start` to `b` (through `m` for a curve), if it can build (`placeWall`).
    private func placeWall(to b: SIMD3<Double>, through m: SIMD3<Double>?) {
        guard let kit, let s = start else { return }
        let a = world(s)
        guard let r = try? kit.checkWall(a: [a.x, a.y, a.z], b: [b.x, b.y, b.z], m: m.map { [$0.x, $0.y, $0.z] }) else { return }
        if !r.ok() {
            // a curve too tight keeps its ends: the next click tries a wider bend
            if m != nil && r.reason().contains("too tight") { showGhostMessage(r.reason()); return }
            clearPreview()           // (clears a curve's live message first)
            message = "couldn’t place that wall: \(r.reason())"
            return
        }
        message = ""
        let wall = DrawnWall.wall(from: s, to: local(b), through: m.map(local))
        session.perform("Draw Wall") { session.drawnWalls.append(wall) }
        clearPreview()
        sync()
    }

    /// Stand a sway brace on the upright face the user clicked: the one its preview showed (`placeSway`).
    private func placeBrace(_ hit: Surface) {
        braceRequest = nil
        guard let kit,
              let spot = try? kit.braceAt(face: UInt64(hit.face), point: [hit.point.x, hit.point.y, hit.point.z], sweep: sweep,
                                          autoBuild: session.finsOn && autoLike ? build?.latest : nil, removed: removedIDs()) else { return }
        dropGhost()
        guard spot.ok() else {
            message = "couldn’t place that brace: \(spot.reason())"
            return
        }
        message = ""
        let p = spot.point()
        let wall = DrawnWall.brace(at: local(SIMD3(p[0], p[1], p[2])), face: hit.face, capped: !spot.full())
        session.perform("Add Sway Brace") { session.drawnWalls.append(wall) }
        sync()
    }

    // MARK: selecting, removing

    /// A click on a placed support selects it; a second click deselects (`selectWall`).
    func select(_ id: UUID) {
        selectedID = selectedID == id ? nil : id
        message = ""
        syncSelection()
        refresh()
    }

    /// One readout line naming what is selected and how to remove it (`selectedNote`).
    var selectedNote: String? {
        guard let w = selected else { return nil }
        let o = outcomes[w.id]
        let what = w.kind == .sway
            ? JS.t("sway brace {mm}mm tall", ["mm": JS.number(JS.round(o?.height ?? 0))])
            : JS.t("wall {mm}mm long", ["mm": JS.number(JS.round(o?.length ?? 0))])
        let tines = (o?.tines ?? 0) != 0 ? ", " + JS.t("{n} tines", ["n": String(o?.tines ?? 0)]) : ""
        return JS.t("selected: {what}{tines}. Press Delete or Remove selected to take it out (Esc to keep it)", ["what": what, "tines": tines])
    }

    /// Remove selected (`removeSelected`): one undo step.
    func removeSelected() {
        guard let id = selectedID else { return }
        session.perform("Remove Support") { session.drawnWalls.removeAll { $0.id == id } }
        selectedID = nil
        message = ""
        sync()
    }

    /// Clear all (`draw-clear`): the hand-drawn walls, one undo step.
    func clearAll() {
        guard hasWalls else { return }
        session.perform("Clear Walls") { session.drawnWalls = [] }
        message = ""
        clearPreview()
        sync()
    }

    // MARK: keys and cancel

    /// Backspace / Delete (`app.js:63-74`): a wall half drawn takes back its last point; else the selected support goes.
    func key(_ k: ViewerController.HandKey) -> Bool {
        if k == .backspace, drawActive, undoPoint() { return true }
        guard selectedID != nil else { return false }
        removeSelected()
        return true
    }

    /// Edit > Delete, the system's item (the viewer answers its `delete:` while it has the keys): as Backspace.
    func deleteMenu() { _ = key(.backspace) }

    /// Esc, a right click or Ctrl+click in the viewport (`cancelArmed`); Esc also deselects (`app.js:194-205`).
    func cancel(escape: Bool) -> Bool {
        if drawActive, start != nil { clearPreview(); return true }
        if escape, let id = selectedID { select(id); return true }
        return false
    }

    /// The window's Esc cascade.
    func escape() -> Bool { cancel(escape: true) }
}

/// The hand-placed supports' volume and extent, for the fit check and the grams receipt (`activeAdded`).
struct AddedGeometry: Equatable, Sendable {
    var volumeMm3 = 0.0
    var min: SIMD3<Double>?
    var max: SIMD3<Double>?

    init?(json: String) {
        guard let o = try? JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any] else { return nil }
        volumeMm3 = (o["volumeMm3"] as? Double) ?? 0
        // no bounds: nothing was built
        guard let b = o["bounds"] as? [String: Any], let lo = b["min"] as? [Double], let hi = b["max"] as? [Double],
              lo.count == 3, hi.count == 3 else { return nil }
        min = SIMD3(lo[0], lo[1], lo[2])
        max = SIMD3(hi[0], hi[1], hi[2])
    }
}
