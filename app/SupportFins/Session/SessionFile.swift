import Foundation
import SupportFinsCore

/// The session a 3MF export carries and what restoring one asks of the window (`web/ui/session.js`, docs/FFI.md
/// "Sessions"). The text written is the description the engine's `export` takes: it makes the file's `sessionOf`.
enum SessionFile {
    /// `{ finMode, finsVisible, form: [[id, value], ...], walls, removed }` for one model. The form keeps `FORM_IDS` order;
    /// the removed keys are sorted (the web's `Set` keeps the order they were clicked in, which this app does not).
    static func json(settings: FinSettings, finsOn: Bool, walls: [DrawnWall], removed: Set<FinKey>) -> String {
        func q(_ s: String) -> String { String(decoding: (try? JSONEncoder().encode(s)) ?? Data("\"\"".utf8), as: UTF8.self) }
        let form = settings.sessionForm.map { id, v -> String in
            switch v {
            case .flag(let b): "[\(q(id)),\(b)]"
            case .text(let t): "[\(q(id)),\(q(t))]"
            }
        }.joined(separator: ",")
        return "{\"finMode\":\(q(settings.mode.rawValue)),\"finsVisible\":\(finsOn),\"form\":[\(form)],\"walls\":\(walls.json),"
            + "\"removed\":[\(removed.map(\.raw).sorted().map(q).joined(separator: ","))]}"
    }

    /// The restored hand-placed supports as the window holds them (`restoreSession`: `vec(w.a)`, `nearestFace`).
    static func walls(_ r: SessionRestore) -> [DrawnWall] {
        func v(_ p: [Double]) -> SIMD3<Double> { SIMD3(p[0], p[1], p[2]) }
        return r.walls.map { w in
            w.sway ? .brace(at: v(w.a), face: Int(w.face), capped: w.capped)
                   : .wall(from: v(w.a), to: v(w.b), through: w.m.map(v))
        }
    }

    /// `restoreState` for a session onto the part just shown: the settings and mode (unless the part is beside a plate's
    /// models, which keep theirs), fins on or off, the hand-placed supports and the removed fins. The pose is the
    /// identity, as `restoreSession` resets it, and hand-placement is off. Nothing is an undo step: a new part starts
    /// its history here (`resetHistory`), and a part added beside a plate goes on with the plate's.
    @MainActor
    static func apply(_ r: SessionRestore, to session: PartSession) {
        var settings = session.settings
        settings.apply(session: r.form)
        if let mode = r.finMode.flatMap(FinSettings.Mode.init(rawValue:)) { settings.mode = mode }
        session.settings = settings
        if let on = r.finsVisible { session.finsOn = on }
        session.drawnWalls = walls(r)
        session.addByHand = false
        session.removedFins = Set(r.removed.map { FinKey(raw: $0) })
    }
}

extension PartSession {
    /// The session of the model on show, for a 3MF export (`sessionOf` over the live model, `ui/session.js`).
    var sessionJSON: String {
        SessionFile.json(settings: settings, finsOn: finsOn, walls: drawnWalls, removed: removedFins)
    }
}
