import Foundation
import SupportFinsCore

/// The hand-placed supports as the readout and the export read them.
struct DrawReadoutState: Equatable, Sendable {
    var input: DrawReadoutInput
    /// The volume and extent the swept supports add to the plate (nil: none built).
    var added: AddedGeometry?
}

extension DrawModel {
    /// What `ui/readout.js` reads of `ui/walls.js`: the swept walls (`w.ok`, `w.info`), `drawMsg`, the selection's note,
    /// the shape and whether the supports are shown. A wall the sweep has not reached yet is left out of the counts.
    var readoutState: DrawReadoutState? {
        _ = sweepRevision
        guard session.finsOn || session.settings.mode == .draw else { return nil }
        var input = DrawReadoutInput()
        input.shown = drawShown
        input.shape = shape.rawValue
        input.msg = message
        input.selectedNote = selectedNote
        input.walls = session.drawnWalls.compactMap { w in
            guard let o = outcomes[w.id] else { return nil }
            return DrawReadoutInput.Wall(kind: w.kind.rawValue, ok: o.ok, tines: o.tines, braces: o.ribs, cross: o.cross, stilt: o.stilt, reason: o.reason)
        }
        return DrawReadoutState(input: input, added: drawShown ? added : nil)
    }

    /// The sweep File > Export writes with the fins and the pad, and whether it is being re-stood (`activeAdded`).
    var exportDrawn: ExportModel.Drawn {
        _ = sweepRevision
        guard session.finsOn, drawShown, !session.drawnWalls.isEmpty else { return ExportModel.Drawn(sweep: nil, pending: false) }
        let current = sweepCurrent && sweptIDs == session.drawnWalls.map(\.id)
        return current ? ExportModel.Drawn(sweep: sweep, pending: false) : ExportModel.Drawn(sweep: nil, pending: true)
    }

    /// The live model's walls for a plate export, now: the landed sweep when it is of the walls and the pose on show,
    /// else stood here (an export does not wait on the background sweep). Nil when none are shown.
    func sweepNow() -> DrawnSweep? {
        guard session.finsOn, drawShown, !session.drawnWalls.isEmpty, let core = session.parts.first else { return nil }
        if sweepCurrent && sweptIDs == session.drawnWalls.map(\.id) { return sweep }
        let s = session.settings
        guard let kit = try? DrawKit(part: core.part, rot: session.pose.columnMajor, thresholdDeg: s.threshold,
                                     settingsJson: s.drawSettingsJSON(), tunablesJson: s.tunablesJSON()) else { return nil }
        let auto = autoLike ? host.wiring?.build?.latest : nil
        let ids = auto == nil ? [] : (host.wiring?.build?.matchedRemovals(of: session.removedFins).keys.sorted().map { UInt64($0) } ?? [])
        return try? kit.sweep(wallsJson: session.drawnWalls.json, autoBuild: auto, removed: ids)
    }
}
