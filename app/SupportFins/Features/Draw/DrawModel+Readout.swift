import Foundation

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
}
