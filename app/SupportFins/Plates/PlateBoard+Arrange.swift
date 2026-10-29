import SwiftUI
import SupportFinsCore
import Viewer
import simd

/// Models that collide, and Arrange (`ui/arrange.js`, local issue 052). Each model of the plate being edited has a
/// footprint (`Footprint`): the 1 mm cells its part, fins and pad cover at any height. Where two share a cell they
/// would print into each other: those cells show red on the bed, a banner over the view offers Arrange, and the
/// Export menu says so. A drag never makes one (Move stops it short), but a turn, a fin rebuild or an undo can. The
/// export is not held back -- a slicer shows the same clash, and the user may mean it.
extension PlateBoard {
    /// The plate's models with their footprints, empty slots left out (`footprints`).
    func footprints() -> [(slot: ModelSlot, grid: Footprint)] {
        plate.models.enumerated().compactMap { k, m in
            guard k == plate.live ? hasLive : m.stash != nil, let g = footprint(of: m) else { return nil }
            return (m, g)
        }
    }

    /// The cells any two of the plate's models share, in plate mm (`checkOverlap`).
    func computeClash() -> [SIMD2<Int64>] {
        guard hasLive, plate.models.count > 1 else { return [] }
        let placed = footprints().map { $0.grid.placed(x: $0.slot.xy.x, y: $0.slot.xy.y) }
        let flat = footprintClashCells(placed: placed)
        return stride(from: 0, to: flat.count - 1, by: 2).map { SIMD2(flat[$0], flat[$0 + 1]) }
    }

    /// The toolbar's Arrange shows with a model on show and more than one on the plate (`syncButton`, `ui/arrange.js`).
    var arrangeShown: Bool { hasLive && modelCount > 1 }

    /// Arrange: pack the plate's models -- the biggest first, each at the free spot nearest the bed's centre, `gap` mm
    /// from the others, turned as they are (a turn about Z would be a new pose and rebuild the fins) -- once every fin
    /// build is in: a model packed without the fins it is getting would collide. One undo step. A model the bed has
    /// no room left for goes past its right edge, where the Volume line says it hangs off. This is the entry point for
    /// the clash banner, "Arrange first" in the Export menu and the toolbar's Arrange (`arrange`, `ui/arrange.js`).
    func arrange() async {
        guard hasLive, !arranging, !busy, !session.dragLocked else { return }
        let plate = self.plate
        let building = host.wiring?.build?.building == true
        let current = await parkedSettled(seconds: 0)
        if building || !current {
            arranging = true
            await host.wiring?.build?.settled()
            _ = await parkedSettled(seconds: 30)
            arranging = false
            // switched, or picked up, meanwhile: that press was for another plate
            guard self.plate === plate, hasLive, !busy, !session.dragLocked else { return }
        }
        let fps = footprints()
        guard fps.count >= 2, let v = host.controller?.volume else { return }
        let at = footprintArrange(grids: fps.map(\.grid), bx: Double(v.x), by: Double(v.y), gap: Self.gap)
        let moves = fps.enumerated().compactMap { k, f -> (slot: ModelSlot, xy: SIMD2<Double>)? in
            let xy = SIMD2(at[2 * k], at[2 * k + 1])
            return f.slot.xy == xy ? nil : (f.slot, xy)
        }
        guard !moves.isEmpty else { return }
        session.pushMoves(moves.map { SlotMove(who: $0.slot.id, xy: $0.slot.xy) }, name: "Arrange")
        placeSlots(moves.map { SlotMove(who: $0.slot.id, xy: $0.xy) })
    }

    /// Put each slot `who` of the plate being edited at its `xy` (an undo of a move, or Arrange) (`placeSlots`).
    func placeSlots(_ moves: [SlotMove]) {
        for mv in moves {
            guard let k = plate.models.firstIndex(where: { $0.id == mv.who }) else { continue }
            if k == plate.live { placeLive(plate.models[k], mv.xy) } else { plate.models[k].xy = mv.xy }
        }
        sync()
    }

    /// Put live slot `m` at `xy`: the world shifts by the difference, the camera with it (`placeLive`, `ui/move.js`).
    func placeLive(_ m: ModelSlot, _ xy: SIMD2<Double>) {
        let d = xy - m.xy
        m.xy = xy
        host.controller?.renderer.camera.target -= SIMD3(Float(d.x), Float(d.y), 0)
    }
}

/// Models of the plate collide (`#clash-banner`): red like the cells, under the plate tabs.
struct ClashBanner: View {
    let board: PlateBoard

    var body: some View {
        if board.clashing {
            HStack(spacing: 12) {
                Text("Models overlap: they would print into each other.")
                Button("Arrange") { Task { await board.arrange() } }
                    .disabled(board.arranging)
                    .accessibilityIdentifier("clash-arrange")
            }
            .font(.system(size: 13))
            .foregroundStyle(Color(red: 1, green: 0.54, blue: 0.54))
            .padding(.vertical, 6).padding(.leading, 14).padding(.trailing, 8)
            .background(.regularMaterial, in: .rect(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Color(red: 1, green: 0.42, blue: 0.42)))
            .shadow(radius: 12)
            .padding(.top, 52)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("clash-banner")
        }
    }
}
