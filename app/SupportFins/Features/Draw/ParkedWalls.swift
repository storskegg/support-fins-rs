import Foundation
import Metal
import SupportFinsCore
import Viewer

/// A parked model's hand-placed supports (`ui/models.js` parks a model with `takeState`, which holds its walls;
/// `ui/parkedbuild.js:134-149` stands them with `sweepDrawn` beside Auto's walls less its removed fins). Kept per model
/// in `ParkedModel.featureStates` (`Feature.modelState`): the walls and the add-by-hand state come back with the model,
/// and while it is parked the walls are re-stood in the background for its pose under the plate's settings, for the plate
/// scene and the footprint.
@MainActor
final class ParkedWalls {
    var walls: [DrawnWall]
    var addByHand: Bool

    private struct Key: Equatable {
        var pose: [Double]
        var threshold: Double
        var settings: String
        var tunables: String
        var walls: [DrawnWall]
        var build: ObjectIdentifier?
        var removed: [UInt64]
    }

    /// What was stood last, and for which request.
    private var done: (key: Key, sweep: DrawnSweep?, mesh: OverlayMesh?)?
    private var asked: Key?
    private var task: Task<Void, Never>?

    init(walls: [DrawnWall], addByHand: Bool) { (self.walls, self.addByHand) = (walls, addByHand) }

    /// The walls stood for `s` under `settings`: what is ready, which may lag a settings change by a moment (the parked
    /// model is drawn pale meanwhile). Starts the work when the request changed; `landed` runs when it is ready.
    func standing(for s: ParkedModel, settings: FinSettings, device: MTLDevice?, landed: @escaping @MainActor () -> Void)
        -> (sweep: DrawnSweep?, mesh: OverlayMesh?) {
        let (key, auto) = request(for: s, settings: settings)
        let removed = key.removed
        if done?.key != key, asked != key {
            asked = key
            task?.cancel()
            let part = s.part.part, json = walls.json
            task = Task { @MainActor [weak self] in
                let made = await Task.detached(priority: .utility) { () -> DrawnSweep? in
                    guard let kit = try? DrawKit(part: part, rot: key.pose, thresholdDeg: key.threshold, settingsJson: key.settings,
                                                 tunablesJson: key.tunables) else { return nil }
                    return try? kit.sweep(wallsJson: json, autoBuild: auto, removed: removed)
                }.value
                guard let self, !Task.isCancelled, self.asked == key else { return }
                var mesh: OverlayMesh?
                if let made, let device {
                    let tris = made.triangles()
                    if tris.count() > 0, let b = tris.metalBuffer(device: device) { mesh = OverlayMesh(buffer: b, vertices: Int(tris.count())) }
                }
                self.done = (key, made, mesh)
                landed()
            }
        }
        return (done?.sweep, done?.mesh)
    }

    /// What `s` under `settings` asks to be stood, and Auto's build it stands clear of (none in Draw).
    private func request(for s: ParkedModel, settings: FinSettings) -> (Key, BuildResult?) {
        let auto = settings.mode != .draw ? s.build.latest : nil
        let removed = auto == nil ? [] : s.build.matchedRemovals(of: s.removedFins).keys.sorted().map { UInt64($0) }
        return (Key(pose: s.pose.columnMajor, threshold: settings.threshold, settings: settings.drawSettingsJSON(),
                    tunables: settings.tunablesJSON(), walls: walls, build: auto.map(ObjectIdentifier.init), removed: removed), auto)
    }

    /// The walls stood for `s` under `settings` now, for an export (`plateGeometry` stands them as it writes): the
    /// background sweep when it is of this request, else stood here.
    func sweepNow(for s: ParkedModel, settings: FinSettings) -> DrawnSweep? {
        let (key, auto) = request(for: s, settings: settings)
        if let done, done.key == key { return done.sweep }
        guard let kit = try? DrawKit(part: s.part.part, rot: key.pose, thresholdDeg: key.threshold, settingsJson: key.settings,
                                     tunablesJson: key.tunables) else { return nil }
        return try? kit.sweep(wallsJson: walls.json, autoBuild: auto, removed: key.removed)
    }
}

extension DrawFeature {
    /// A parked model's re-stood walls, when it has any and the fins are on (`walls.length && (draw || autoLike)` is every
    /// mode).
    func parkedDrawn(_ s: ParkedModel, settings: FinSettings, finsOn: Bool, landed: @escaping @MainActor () -> Void)
        -> (sweep: DrawnSweep?, mesh: OverlayMesh?)? {
        guard finsOn, let p = s.featureStates[ObjectIdentifier(DrawFeature.self)] as? ParkedWalls, !p.walls.isEmpty else { return nil }
        return p.standing(for: s, settings: settings, device: host.controller?.renderer.device, landed: landed)
    }

    /// The walls a plate export writes for model `m` (`PlateBoard.drawn`, `plateGeometry`), in print space about its own
    /// spot: the live model's sweep, or a parked one's stood for its pose under its plate's settings. Nil without walls.
    func exportDrawn(_ m: ModelSlot, board: PlateBoard) -> DrawnSweep? {
        if m === board.plate.liveSlot { return model.sweepNow() }
        guard let s = m.stash, let p = s.featureStates[ObjectIdentifier(DrawFeature.self)] as? ParkedWalls, !p.walls.isEmpty,
              let i = board.plates.firstIndex(where: { $0.models.contains { $0 === m } }) else { return nil }
        let ps = board.plateSettings(i)
        return ps.finsOn ? p.sweepNow(for: s, settings: ps.settings) : nil
    }

    /// The hand-placed walls of model `m` of the plate (`gs[0].session()`'s `walls`): the live model's are the window's, a
    /// parked one's are kept with it.
    func wallsOf(_ m: ModelSlot, board: PlateBoard) -> [DrawnWall] {
        if m === board.plate.liveSlot { return host.session.drawnWalls }
        return (m.stash?.featureStates[ObjectIdentifier(DrawFeature.self)] as? ParkedWalls)?.walls ?? []
    }

    /// `takeState`: the walls and the add-by-hand state go with the model when it is parked.
    func modelState() -> Any? {
        let s = host.session
        return s.drawnWalls.isEmpty && !s.addByHand ? nil : ParkedWalls(walls: s.drawnWalls, addByHand: s.addByHand)
    }

    /// `restoreState`: they come back with it, as the state of a model put on show (not an undo step).
    func restoreModelState(_ state: Any) {
        guard let p = state as? ParkedWalls else { return }
        host.session.drawnWalls = p.walls
        host.session.addByHand = p.addByHand
        model.sync()
    }
}
