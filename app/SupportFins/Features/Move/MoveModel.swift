import Foundation
import Observation
import SupportFinsCore
import Viewer
import simd

/// Move (`web/ui/move.js`): models slide across their plate. On (the toolbar's Move, Model > Move, or M; Esc leaves
/// it), a drag that starts on a model moves it -- the one being edited or a parked one -- and a drag anywhere else
/// still orbits. Fins are built in the model's own print space, so a move rebuilds nothing: the model's spot on the
/// plate (its slot's `xy`) is all that changes. The live model stays at the origin where every tool expects it;
/// moving it shifts the bed and the parked models the other way (`PlateBoard.sync`), and the camera with them, so on
/// screen the bed stays put and the model moves. A drag never pushes one model into another (`footprintDragStep`): it
/// stops where they would touch. Each drag is one step of the plate's undo.
@MainActor
final class MoveModel {
    weak let host: ViewerModel?
    /// The window's session: kept, not reached through `host`, so a drag ending as the window closes still finds it.
    private let session: PartSession
    private var board: PlateBoard? { host?.features[PlatesFeature.self]?.board }
    private var tool: MoveTool? { host?.controller?.move }

    /// A drag in progress (`drag` in `ui/move.js`): the slot, its spot at the press, and the plate point pressed.
    private struct Drag {
        var slot: ModelSlot
        var from: SIMD2<Double>
        var grab: SIMD2<Double>
        var moved = false
        /// The other plate it is over (`plateUnder`), and its spot there.
        var over: (plate: Int, xy: SIMD2<Double>)?
    }
    private var drag: Drag?
    /// The tool is being switched off: a drag ends where it stands and moves nothing to another plate.
    private var leaving = false

    init(host: ViewerModel) {
        self.host = host
        session = host.session
        guard let tool = host.controller?.move else { return }
        tool.onBegin = { [weak self] pick, world in self?.begin(pick, world) ?? false }
        tool.onDrag = { [weak self] world in self?.dragged(world) }
        tool.onEnd = { [weak self] in self?.end() }
        tool.onToggleKey = { [weak self] in self?.toggle() }
        host.session.slotSpot = { [weak self] id in self?.board?.plate.models.first { $0.id == id }?.xy }
        host.session.placeSlots = { [weak self] moves in self?.place(moves) }
    }

    var isOn: Bool { tool?.isOn ?? false }

    func toggle() { setOn(!isOn) }

    /// `setMove`: on needs a model (`moving = !!v && !!part`); turning it off ends a drag where it stands. One pointer
    /// mode at a time: Lay a face flat and Remove fins, which wait on a click Move would take, are left.
    func setOn(_ on: Bool) {
        guard let tool else { return }
        let now = on && (board?.hasLive ?? false)
        if now == tool.isOn { return }
        if now { host?.controller?.cancelArmed() }
        leaving = !now
        tool.setOn(now)
        leaving = false
    }

    // MARK: a drag

    private func begin(_ pick: ModelPick, _ world: SIMD2<Double>) -> Bool {
        guard let board, !board.busy else { return false }
        let slot: ModelSlot
        switch pick {
        case .live: slot = board.plate.liveSlot
        case .parked(let i):
            guard board.parkedSlots.indices.contains(i) else { return false }
            slot = board.parkedSlots[i]
        }
        drag = Drag(slot: slot, from: slot.xy, grab: world + board.plate.liveSlot.xy)
        session.dragLocked = true
        return true
    }

    private func dragged(_ world: SIMD2<Double>) {
        guard var d = drag, let board else { return }
        let round = { (v: Double) in (v * 10).rounded() / 10 }
        let at = world + board.plate.liveSlot.xy
        let to = SIMD2(round(d.from.x + at.x - d.grab.x), round(d.from.y + at.y - d.grab.y))
        if !d.moved && hypot(to.x - d.from.x, to.y - d.from.y) < 0.5 { return }
        // never into another model: it stops short, or slides along the other's edge
        let xy = step(d.slot, to: to, on: board)
        if xy == d.slot.xy { return }
        d.moved = true
        d.over = over(xy, on: board)
        drag = d
        put(d.slot, at: xy, on: board)
        lightTarget(d.over?.plate)
    }

    /// The other plate a model at `xy` (the edited plate's coordinates) would be dropped on: its middle over that
    /// plate's bed (`over`, `plateUnder`, `ui/move.js:170-174`, `ui/platerow.js`).
    private func over(_ xy: SIMD2<Double>, on board: PlateBoard) -> (plate: Int, xy: SIMD2<Double>)? {
        guard let v = host?.controller?.volume else { return nil }
        // an engine refusal reads as "over no plate": the model is not handed to another plate on a guess
        guard let r = try? plateUnder(x: xy.x, y: xy.y, active: UInt64(board.active), count: UInt64(board.plates.count),
                                      pitch: board.pitch, bx: Double(v.x), by: Double(v.y)) else { return nil }
        return r.count == 3 ? (Int(r[0]), SIMD2(r[1], r[2])) : nil
    }

    /// That plate's bed lit while the model is over it (`markTarget`, `ui/plateview.js`).
    private func lightTarget(_ plate: Int?) {
        guard let renderer = host?.controller?.renderer, renderer.plateScene.target != plate else { return }
        var scene = renderer.plateScene
        scene.target = plate
        renderer.setPlateScene(scene)
    }

    /// `dragStep` with `clashAt` as the probe: the others stand where they stand.
    private func step(_ slot: ModelSlot, to: SIMD2<Double>, on board: PlateBoard) -> SIMD2<Double> {
        guard let grid = board.footprint(of: slot) else { return to }
        do {
            let others = try board.plate.models.compactMap { o in
                try o === slot ? nil : board.footprint(of: o)?.placed(x: o.xy.x, y: o.xy.y)
            }
            let r = try footprintDragStep(at: [slot.xy.x, slot.xy.y], to: [to.x, to.y], grid: grid, others: others)
            return SIMD2(r[0], r[1])
        } catch {
            // the engine refused (a position far off the plate): the model stays where it is rather than jump
            host?.status = error.localizedDescription
            return slot.xy
        }
    }

    /// Put `slot` at `xy`. The live one shifts the world the other way and the camera with it (`placeLive`); a parked
    /// one only moves, and the scene is laid out again.
    private func put(_ slot: ModelSlot, at xy: SIMD2<Double>, on board: PlateBoard) {
        let d = xy - slot.xy
        slot.xy = xy
        if slot === board.plate.liveSlot, let r = host?.controller?.renderer {
            r.camera.target.x -= Float(d.x)
            r.camera.target.y -= Float(d.y)
        }
        board.sync()
    }

    /// The drag ends. Dropped over another plate, the model goes to it, with an Undo in the toast (`moveToPlate`; the
    /// switch empties the page a moment, which would turn Move off: it stays on); else it stays, one undo step.
    private func end() {
        session.dragLocked = false
        guard let d = drag, let board else { drag = nil; return }
        drag = nil
        if let renderer = host?.controller?.renderer, renderer.plateScene.target != nil { lightTarget(nil) }
        guard d.moved else { return }
        if let to = d.over, !leaving, board.moveToPlate(d.slot, to: to.plate, at: to.xy, back: d.from) {
            setOn(true)
            return
        }
        session.pushMoves([SlotMove(who: d.slot.id, xy: d.from)], name: JS.t("Move Model"))
    }

    /// An undo or redo of a move: the slots back at their spots, edited or parked (`placeSlots`).
    private func place(_ moves: [SlotMove]) {
        guard let board else { return }
        for m in moves {
            guard let slot = board.plate.models.first(where: { $0.id == m.who }) else { continue }
            put(slot, at: m.xy, on: board)
        }
    }
}
