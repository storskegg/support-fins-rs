import Foundation
import Viewer
import simd

/// Plates (`ui/plates.js`): several in one window, each with its models, its fin settings and its undo, in a row on
/// screen; a click on one, there or on its tab, makes it the one being edited. A switch parks the live model as it
/// stands and brings the other plate's back, so nothing is re-read or re-welded. A deleted plate is kept for a few
/// seconds behind the Undo toast before it is freed.
extension PlateBoard {
    /// Centre to centre between plates along X (`pitch`, `ui/plateview.js`).
    static func pitch(volumeX x: Double) -> Double { x + max(20, 0.12 * x) }
    var pitch: Double { Self.pitch(volumeX: Double(host.controller?.volume.x ?? PrinterVolume.standard.x)) }

    // MARK: what the tabs say

    /// The name a plate's tab shows: the live model's, or none when the plate is empty (`render`, `ui/plates.js`).
    func tabName(_ i: Int) -> String {
        guard plates.indices.contains(i) else { return "" }
        if i == active { return hasLive ? (session.parts.first?.info.name ?? "") : "" }
        return plates[i].liveSlot.stash?.name ?? ""
    }

    /// How many models plate `i` holds.
    func tabCount(_ i: Int) -> Int {
        guard plates.indices.contains(i) else { return 0 }
        let p = plates[i]
        return p.models.enumerated().filter { k, m in i == active && k == p.live ? hasLive : m.stash != nil }.count
    }

    /// No model on any plate yet.
    var isEmpty: Bool { plates.indices.allSatisfy { tabCount($0) == 0 } }

    static func stem(_ name: String) -> String {
        name.replacing(/\.(stl|3mf|step|stp|obj)$/.ignoresCase(), with: "")
    }

    // MARK: park and unpark a plate

    /// Put the live model and the plate's settings and undo away, leaving the window empty (`park`, `ui/models.js`).
    func parkPlate(_ plate: Plate) {
        swapping = true
        defer { swapping = false }
        guard hasLive else { parkModel(plate); return }
        plate.history = session.takeHistory()
        plate.settings = PlateSettings(settings: session.settings, finsOn: session.finsOn)
        parkModel(plate)
    }

    /// Bring `plate`'s live slot back under the plate's settings; the window is empty (`unpark`, `ui/models.js`): a
    /// model put back onto a plate that had none keeps its own, and an empty plate's next model takes them too.
    func unparkPlate(_ plate: Plate) {
        swapping = true
        defer { swapping = false }
        if let own = plate.settings ?? plate.liveSlot.stash?.settings {
            session.settings = own.settings
            session.finsOn = own.finsOn
        }
        unparkModel(plate)
        if let h = plate.history { session.setHistory(h); plate.history = nil }
    }

    /// Edit plate `i`, whose models are parked (or which is empty); the window is empty (`show`).
    func show(_ i: Int) {
        active = i
        unparkPlate(plates[i])
        sync()
        scheduleParked()
    }

    // MARK: switching

    /// Switch to plate `i` (a tab); the camera glides along the row.
    func switchPlate(_ i: Int) {
        guard !busy else { return }
        go(i)
    }

    private func go(_ i: Int, quiet: Bool = false) {
        guard i != active, plates.indices.contains(i), let controller = host.controller else { return }
        let from = plate.liveSlot.xy, camera = controller.renderer.camera
        parkPlate(plate)
        arrive(i, from: from, camera: camera, quiet: quiet)
    }

    /// Edit plate `i`, the one being edited already parked, its live model having stood at `from`: the camera keeps
    /// its view of the row, then glides unless `quiet` (`arrive`).
    func arrive(_ i: Int, from: SIMD2<Double>, camera: OrbitCamera, quiet: Bool) {
        let dx = Double(i - active) * pitch
        show(i)
        guard !quiet, let controller = host.controller else { return }
        // the row slid under the camera by the plates' spacing and the two live models' spots: start from the same view
        let to = plate.liveSlot.xy
        let shift = SIMD3<Float>(Float(dx + to.x - from.x), Float(to.y - from.y), 0)
        controller.glideCamera(fromTarget: camera.target - shift, fromEye: camera.eye - shift)
    }

    /// A new, empty plate, edited (`addPlate`).
    func addPlate() {
        guard !busy else { return }
        plates.append(Plate())
        go(plates.count - 1)
    }

    /// An import that should not replace a model: a new plate unless this one is empty (`freePlate`).
    func freePlate() {
        if hasLive { addPlate() }
    }

    /// A multi-file import whose last file did not load leaves its new plate empty (`dropEmptyPlate`).
    func dropEmptyPlate() {
        guard !hasLive, plates.count > 1, plate.models.count == 1 else { return }
        plates.remove(at: active)
        show(min(active, plates.count - 1))
    }

    /// Pull the camera back to take in every plate of the row (`frameAll`, `ui/plateview.js`).
    func frameAll() {
        guard let controller = host.controller else { return }
        let v = controller.volume, p = Float(pitch), n = Float(plates.count)
        let was = controller.renderer.camera
        controller.renderer.camera.frame(size: SIMD3((n - 1) * p / 2 + v.x / 2, v.y / 2, 0))
        let live = plate.liveSlot.xy
        let cx = ((n - 1) / 2 - Float(active)) * p - Float(live.x)
        controller.renderer.camera.target = SIMD3(cx, Float(-live.y), 0)
        controller.glideCamera(fromTarget: was.target, fromEye: was.eye)
    }

    // MARK: delete, with Undo

    /// Delete plate `i`: its models go, and the toast's Undo brings them back (`deletePlate`).
    func deletePlate(_ i: Int) {
        guard !busy, plates.indices.contains(i) else { return }
        if moved?.from === plates[i] { expire() }       // its Undo would have nowhere to go
        if i == active { parkPlate(plates[i]) }
        let entry = plates[i]
        let slots = entry.models.filter { $0.stash != nil }
        let own = (settings: entry.settings, history: entry.history)
        if plates.count == 1 {
            // the one plate stays, emptied (the same plate, for an earlier Undo to find)
            entry.models = [ModelSlot()]
            entry.live = 0
            entry.settings = nil
            entry.history = nil
            show(0)
            trashed(slots, plate: entry, index: 0, text: removedText(slots), own: own)
            return
        }
        plates.remove(at: i)
        if i == active {
            show(min(i, plates.count - 1))
        } else {
            if i < active { active -= 1 }
            sync()
        }
        trashed(slots, plate: nil, index: i, text: "Deleted plate \(i + 1)", own: own)
    }

    /// Bring the last removal back: to its plate if that is still there (into its slot when the plate is empty,
    /// beside its models when not), else as a new plate where it was (`undoDelete`).
    func undoDelete() {
        guard let t = trash, !busy else { return }
        t.timer.cancel()
        trash = nil
        toast = nil
        var i = t.plate.flatMap { p in plates.firstIndex { $0 === p } }
        // the plate was deleted with them (plate nil), or later: a new one where it was
        let fresh = i == nil
        if fresh {
            let at = min(t.index, plates.count)
            plates.insert(Plate(), at: at)
            if at <= active { active += 1 }
            i = at
        }
        guard let i else { return }
        parkPlate(plate)
        let p = plates[i]
        if p.models.count == 1 && p.models[0].stash == nil {
            p.models = t.slots
            // a deleted plate's models come back under its settings and undo
            if let own = t.own { p.settings = own.settings; p.history = own.history }
        } else {
            putBack(p, t.slots)
        }
        p.live = p.models.firstIndex { $0 === t.slots[0] } ?? 0
        show(i)
    }

    // MARK: move to another plate

    /// Move slot `m` (a model of any plate, usually the one being edited) to plate `j` at `xy` (a drag dropped there),
    /// or the free spot nearest the centre when a model there covers it. Plate `j` is then edited, `m` live, under its
    /// settings: its fins rebuild to them, and its removals and pose go with it. Its undo steps stay in the plate it
    /// left, passed over there while it is gone. The toast's Undo takes it back to `back` (its spot before the drag)
    /// unless `back` is nil (`moveToPlate`, `ui/plates.js`).
    @discardableResult
    func moveToPlate(_ m: ModelSlot, to j: Int, at xy: SIMD2<Double>, back: SIMD2<Double>?) -> Bool {
        guard let si = plates.firstIndex(where: { $0.models.contains { $0 === m } }), plates.indices.contains(j),
              let controller = host.controller else { return false }
        let src = plates[si], dst = plates[j]
        guard let k = src.models.firstIndex(where: { $0 === m }), !busy, src !== dst,
              si == active && k == src.live ? hasLive : m.stash != nil else { return false }
        expire()
        let from = plate.liveSlot.xy, camera = controller.renderer.camera
        parkPlate(plate)
        let name = m.stash?.name ?? ""
        src.models.remove(at: k)
        if src.models.isEmpty { src.models = [ModelSlot()] }
        if src.live > k || (src.live == k && k > 0) { src.live -= 1 }       // as Remove model goes on
        m.xy = xy
        let bare = dst.models.count == 1 && dst.models[0].stash == nil
        // its own settings only on a plate that has none yet (`unparkPlate`)
        let own = bare && dst.settings == nil
        if bare { dst.models = [m] } else { putBack(dst, [m]) }
        dst.live = dst.models.firstIndex { $0 === m } ?? 0
        arrive(j, from: from, camera: camera, quiet: false)
        guard let back else { return true }
        let timer = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.undoSeconds))
            guard !Task.isCancelled else { return }
            self?.expire()
        }
        moved = Moved(slot: m, from: src, xy: back, timer: timer)
        toast = own ? "Moved \(name) to plate \(j + 1)" : "Moved \(name) to plate \(j + 1): it takes that plate's settings"
        return true
    }

    /// The toast's Undo of a move: the model back to the plate it left, where it was (or the free spot nearest, if
    /// a model has been put there since), from whichever plate is being edited -- one glide (`undoMove`).
    func undoMove() {
        guard let mv = moved, !busy else { return }
        expire()
        if let to = plates.firstIndex(where: { $0 === mv.from }) { moveToPlate(mv.slot, to: to, at: mv.xy, back: nil) }
    }

    // MARK: clicks on the row

    /// What a click on the view lands on (`plateAt`): a parked model of the plate being edited, another plate's model,
    /// or the bare bed of a plate of the row.
    func tapped(x: Int, y: Int, width: Int, height: Int) {
        guard !busy, tabsMatter, let renderer = host.controller?.renderer else { return }
        Task { [weak self] in
            let pick = await renderer.pickModel(x: x, y: y, width: width, height: height)
            guard let self else { return }
            switch pick {
            case .live: return
            case .parked(let i):
                guard parkedSlots.indices.contains(i), parkedPlates.indices.contains(i) else { return }
                let pi = parkedPlates[i], slot = parkedSlots[i]
                guard let k = plates[pi].models.firstIndex(where: { $0 === slot }) else { return }
                if pi == active { select(k) } else {
                    // a model on another plate: that plate, with that model live
                    plates[pi].live = k
                    switchPlate(pi)
                }
            case nil:
                guard let at = renderer.groundPoint(x: x, y: y, width: width, height: height), let pi = plate(at: at) else { return }
                switchPlate(pi)
            }
        }
    }

    /// The plate of the row whose bed (and the strip under it, for its label) the scene point `at` is on, if not the
    /// one being edited (`plateAt`'s bed test).
    func plate(at: SIMD2<Float>) -> Int? {
        guard let controller = host.controller else { return nil }
        let v = controller.volume, p = Float(pitch)
        let live = plate.liveSlot.xy
        let x0 = at.x + Float(live.x), y = at.y + Float(live.y)
        let i = Int((x0 / p).rounded()) + active
        guard plates.indices.contains(i) else { return nil }
        let x = x0 - Float(i - active) * p
        return abs(x) <= v.x / 2 && abs(y) <= v.y / 2 + 30 ? i : nil
    }

    /// There is something to click: several plates or several models.
    var tabsMatter: Bool { plates.count > 1 || modelCount > 1 }
}
