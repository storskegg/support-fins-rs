import AppKit
import Observation
import SupportFinsCore
import Viewer
import simd

/// Several models on one plate (`ui/models.js`, `ui/plates.js`). A plate holds a list of model slots, one of them
/// live -- the one every tool works on, shown in the window as it always was -- and the rest parked, drawn where they
/// stand (`PlateScene`). A click on a parked model swaps it in the way a plate switch does: the live one is stashed
/// as it stands (its mesh, pose, removed fins and the build that holds its fins) and the clicked one brought back.
///
/// The fin settings (every field, the mode, fins on or off) are the plate's, not a model's: models on one plate print
/// in one job. A change made on the live model rebuilds the parked ones' fins in the background (`PlateBoard+Parked`).
/// So is the undo history, one for the plate: a step remembers whose it was (`SessionSnapshot.owner`), and undoing it
/// edits that model again.
///
/// The live model stays at the origin, where every tool and the viewer expect it. A slot's `xy` is where it stands on
/// its plate, and the bed and the parked models are drawn shifted by minus the live one's; the camera moves with the
/// shift, so nothing jumps on screen.
@MainActor
@Observable
final class PlateBoard {
    /// mm Arrange and Add model leave between footprints: pads and brims stay apart (`GAP`, `ui/arrange.js`).
    static let gap: Int64 = 10
    /// How long a removal's Undo stays on offer (`UNDO_MS`, `ui/plates.js`).
    static let undoSeconds = 10.0

    @ObservationIgnored unowned let host: ViewerModel
    private(set) var plates = [Plate()]
    private(set) var active = 0
    /// Export is switching through the models: the board holds still (`busy`, `ui/plates.js`).
    private(set) var busy = false
    /// The text of the Undo toast while a removal can be undone.
    private(set) var toast: String?

    /// The slot of each parked model in the scene, in the scene's order (what a click on one means).
    @ObservationIgnored var parkedSlots: [ModelSlot] = []
    @ObservationIgnored private var trash: Trash?
    /// The board is bringing a model in itself (a swap, an Undo): the features' `partWillShow` is its own.
    @ObservationIgnored private(set) var swapping = false
    @ObservationIgnored private var addedOver: Bool = false
    @ObservationIgnored var parkedTask: Task<Void, Never>?
    @ObservationIgnored var parkedTimer: Task<Void, Never>?
    @ObservationIgnored var parkedBuilding: ParkedModel?

    /// What a removal can bring back.
    private struct Trash {
        var slots: [ModelSlot]
        var plate: Plate
        var timer: Task<Void, Never>
    }

    init(host: ViewerModel) {
        self.host = host
        host.session.isOnPlate = { [weak self] id in self?.plate.models.contains { $0.id == id } ?? true }
        host.session.focus = { [weak self] id in self?.focus(id) }
        host.session.owner = plates[0].liveSlot.id
    }

    var plate: Plate { plates[active] }
    var session: PartSession { host.session }
    /// A model is on show in the window.
    var hasLive: Bool { !session.parts.isEmpty }
    /// How many models the plate being edited holds (the live one counts when it is on show).
    var modelCount: Int { plate.models.enumerated().filter { k, m in k == plate.live ? hasLive : m.stash != nil }.count }

    // MARK: park and unpark

    /// Put the live model and its state away, leaving the window empty (`park`, `ui/models.js`). The plate's undo
    /// history and settings stay with the session: only a plate switch takes them away.
    private func parkModel(_ plate: Plate) {
        let m = plate.liveSlot
        guard let part = session.parts.first, let controller = host.controller else { m.stash = nil; return }
        let settings = PlateSettings(settings: session.settings, finsOn: session.finsOn)
        let build = host.wiring?.detach() ?? BuildModel(part: part.part)
        let stash = ParkedModel(part: part, openedName: host.openedName, pose: session.pose, removedFins: session.removedFins,
                                build: build, offset: controller.renderer.offset, flags: controller.renderer.flagBuffer(part: 0),
                                settings: settings)
        stash.build.onUpdate = { [weak self] in self?.sync() }
        m.stash = stash
        session.install(parts: [], pose: .identity, removedFins: [], owner: nil)
        controller.show([], name: "")
        host.openedName = nil
    }

    /// Bring the plate's live slot back (stashed, or empty); the window is empty (`unpark`, `ui/models.js`).
    private func unparkModel(_ plate: Plate) {
        let m = plate.liveSlot
        guard let s = m.stash, let controller = host.controller else {
            session.install(parts: [], pose: .identity, removedFins: [], owner: m.id)
            return
        }
        host.features.partWillShow()
        session.install(parts: [s.part], pose: s.pose, removedFins: s.removedFins, owner: m.id)
        controller.rotation = s.pose.float3x3
        host.wiring?.attach(reusing: s.build)
        controller.show([s.part], name: s.name)
        // the seating and the colours it had: no flash of the stand-in before the analysis lands
        controller.renderer.setPose(rotation: s.pose.float3x3, offset: s.offset)
        if let flags = s.flags { controller.renderer.setFlags(flags) }
        host.openedName = s.openedName
        m.stash = nil
        host.features.partDidShow([LoadedPart(name: s.name, mesh: s.part)])
    }

    /// Swap the live model for slot `k` of the plate being edited. The world shifts by the difference of their
    /// spots: the camera goes with it, then pans to the model (`swap`, `ui/models.js`).
    func swap(to k: Int, quiet: Bool = false) {
        guard let controller = host.controller, plate.models.indices.contains(k) else { return }
        let from = plate.liveSlot.xy, to = plate.models[k].xy
        let camera = controller.renderer.camera
        swapping = true
        parkModel(plate)
        plate.live = k
        unparkModel(plate)
        swapping = false
        sync()
        scheduleParked()
        if !quiet { glide(from: camera, shifted: from - to) }
    }

    /// The view as it was, the world moved by `d` under it, aimed at the model now on show; then eased to there.
    private func glide(from camera: OrbitCamera, shifted d: SIMD2<Double>) {
        guard let controller = host.controller else { return }
        let shift = SIMD3<Float>(Float(d.x), Float(d.y), 0)
        var aimed = camera
        aimed.target = SIMD3(0, 0, camera.target.z)
        controller.renderer.camera = aimed
        controller.glideCamera(fromTarget: camera.target + shift, fromEye: camera.eye + shift)
    }

    /// Edit model `k` of the plate being edited (a click on it, `select`).
    func select(_ k: Int) {
        guard !busy, k != plate.live, plate.models.indices.contains(k) else { return }
        swap(to: k)
    }

    /// Edit the model an undo step belongs to, if it is still on the plate (`focusSlot`).
    private func focus(_ id: UUID) {
        guard let k = plate.models.firstIndex(where: { $0.id == id }), k != plate.live else { return }
        swap(to: k)
    }

    // MARK: Add model

    /// A part is about to be shown beside the plate's models (`addSlot`, `ui/models.js`): the live one is parked,
    /// and the plate's undo history set aside, for `session.replaceParts` starts a fresh one.
    func willAdd() {
        guard hasLive else { return }
        addedOver = true
        plate.history = session.takeHistory()
        swapping = true
        parkModel(plate)
        swapping = false
    }

    /// The part is on show: give it a slot of its own at the free spot nearest the bed's centre, and go on with the
    /// plate's undo history past it (`joinPlate`).
    func shown(_ part: CorePart?) {
        defer { sync(); scheduleParked() }
        guard addedOver, let part else { session.owner = plate.liveSlot.id; return }
        addedOver = false
        let slot = ModelSlot(xy: spot(for: part))
        plate.models.append(slot)
        plate.live = plate.models.count - 1
        session.owner = slot.id
        if let h = plate.history { session.setHistory(h); plate.history = nil }
    }

    /// Where a new model of `part` goes (`spot`, `ui/models.js`): the free spot nearest the bed's centre, `gap` mm
    /// from the plate's parked models, or past their right edge when the bed has none.
    private func spot(for part: CorePart) -> SIMD2<Double> {
        guard let controller = host.controller else { return .zero }
        // as it will stand: seated for the identity pose, centred on its spot; its fins are still to come
        let o = seatingOffset(bounds: part.bounds, rotation: matrix_identity_float3x3)
        let grid = try? footprintOfPart(part: part.part, rot: PoseAccess.identity, offset: [Double(o.x), Double(o.y), Double(o.z)])
        let v = controller.volume
        let at = footprintSpot(others: placedFootprints(skipping: []), grid: grid, bx: Double(v.x), by: Double(v.y), gap: Self.gap)
        return SIMD2(at[0], at[1])
    }

    // MARK: Remove model

    /// Remove model: the live one goes, the plate stays (empty, with no other); the toast's Undo puts the model back
    /// (`removeModel`, `ui/plates.js`).
    func removeModel() {
        guard !busy, hasLive else { return }
        let m = takeLive()
        trashed([m], text: removedText([m]))
    }

    /// Take the live model off the plate being edited: its slot, parked, for an Undo to put back. A plate of several
    /// goes on with the model before it; the last one leaves the plate empty (`takeLive`).
    private func takeLive() -> ModelSlot {
        let k = plate.live, m = plate.models[k]
        if plate.models.count == 1 {
            swapping = true
            parkModel(plate)
            swapping = false
            plate.models = [ModelSlot()]
            plate.live = 0
            session.owner = plate.liveSlot.id
            sync()
            return m
        }
        swap(to: k == 0 ? 1 : k - 1)
        plate.models.remove(at: k)
        if plate.live > k { plate.live -= 1 }
        sync()
        return m
    }

    private func removedText(_ slots: [ModelSlot]) -> String {
        slots.count == 1 ? "Removed \(slots[0].stash?.name ?? "model")" : "Removed \(slots.count) models"
    }

    // MARK: the toast's Undo

    private func trashed(_ slots: [ModelSlot], text: String) {
        guard !slots.isEmpty else { return }       // an empty plate: nothing to bring back (the last Undo stays)
        expire()
        let timer = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.undoSeconds))
            guard !Task.isCancelled else { return }
            self?.expire()
        }
        trash = Trash(slots: slots, plate: plate, timer: timer)
        toast = text
    }

    /// The Undo goes, and with it what the toast kept: the models are freed.
    func expire() {
        trash?.timer.cancel()
        trash = nil
        toast = nil
    }

    /// The toast's Undo: bring the last removal back, beside the plate's models (into its slot when the plate is
    /// empty) (`undoDelete`).
    func undoToast() {
        guard let t = trash, !busy else { return }
        t.timer.cancel()
        trash = nil
        toast = nil
        guard let i = plates.firstIndex(where: { $0 === t.plate }) else { return }
        swapping = true
        if hasLive { parkModel(plate) }
        swapping = false
        let p = plates[i]
        if p.models.count == 1 && p.models[0].stash == nil {
            p.models = t.slots
        } else {
            putBack(p, t.slots)
        }
        p.live = p.models.firstIndex { $0 === t.slots[0] } ?? 0
        swapping = true
        unparkModel(p)
        swapping = false
        sync()
        scheduleParked()
    }

    /// Put the parked `slots` (a removal's Undo) back on `plate` beside its models: each where it stood, unless
    /// something put there since covers that spot -- then at the free spot nearest the centre, as Add model would
    /// (`putBack`, `ui/models.js`).
    private func putBack(_ plate: Plate, _ slots: [ModelSlot]) {
        guard let v = host.controller?.volume else { plate.models += slots; return }
        for m in slots {
            let grid = footprint(of: m)
            let near = grid?.placed(x: m.xy.x, y: m.xy.y).dilate(r: Self.gap)
            let clash = near.map { near in
                plate.models.contains { o in
                    o.stash != nil && (footprint(of: o).map { $0.placed(x: o.xy.x, y: o.xy.y).sharedCount(other: near) > 0 } ?? false)
                }
            } ?? false
            if clash {
                let at = footprintSpot(others: placedFootprints(skipping: [m], in: plate), grid: grid, bx: Double(v.x), by: Double(v.y), gap: Self.gap)
                m.xy = SIMD2(at[0], at[1])
            }
            plate.models.append(m)
        }
    }
}
