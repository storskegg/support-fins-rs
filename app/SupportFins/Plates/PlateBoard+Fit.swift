import SupportFinsCore
import Viewer
import simd

/// What the Volume row reads of the plate (`updateFit`, `ui/part.js:472-505`): where the model on show stands, whether a
/// parked one hangs off the bed, and whether any two models of the plate share a cell.
struct PlateFit: Equatable {
    var spot: SIMD2<Double> = .zero
    /// `parkedOff` (`ui/models.js:193`).
    var parkedOff = false
    /// `overlapping` (`ui/arrange.js:97`): computed here from the footprints. The clash PR draws the red cells and the
    /// banner from the same cells (`footprintClashCells`); the row's text is this.
    var clash = false
}

extension PlateBoard {
    /// The plate being edited as `updateFit` sees it. Reads each slot's spot, so a view that calls it follows a drag.
    var fit: PlateFit {
        PlateFit(spot: plate.liveSlot.xy, parkedOff: parkedHangsOff, clash: overlapping)
    }

    /// Two models of the plate share a cell of the bed (`checkOverlap`): only with several models on show.
    var overlapping: Bool {
        guard hasLive, plate.models.count > 1 else { return false }
        let placed = plate.models.compactMap { m in footprint(of: m)?.placed(x: m.xy.x, y: m.xy.y) }
        return placed.count > 1 && !footprintClashCells(placed: placed).isEmpty
    }

    /// A parked model of the plate being edited hangs off the bed (dragged there), its fins counted
    /// (`parkedOff`, `ui/models.js:193-200`).
    var parkedHangsOff: Bool {
        guard let v = host.controller?.volume else { return false }
        let e = 1e-6
        return plate.models.enumerated().contains { k, m in
            guard k != plate.live, let box = parkedBox(m) else { return false }
            return box.x0 < -Double(v.x) / 2 - e || box.x1 > Double(v.x) / 2 + e
                || box.y0 < -Double(v.y) / 2 - e || box.y1 > Double(v.y) / 2 + e
        }
    }

    /// The xy box a parked model and its fins cover on its plate (`footprint(m)`, `ui/models.js:181-189`): its mesh's
    /// box through its pose and seating, unioned with its fins', both about its own spot, then at `xy`.
    func parkedBox(_ m: ModelSlot) -> (x0: Double, y0: Double, x1: Double, y1: Double)? {
        guard let s = m.stash, let device = host.controller?.renderer.device else { return nil }
        let b = s.part.bounds, rot = s.pose.float3x3
        var lo = SIMD2<Float>(repeating: .infinity), hi = SIMD2<Float>(repeating: -.infinity)
        for i in 0..<8 {
            let c = SIMD3<Float>(i & 1 == 0 ? b.min.x : b.max.x, i & 2 == 0 ? b.min.y : b.max.y, i & 4 == 0 ? b.min.z : b.max.z)
            let p = rot * c + s.offset
            lo = simd_min(lo, SIMD2(p.x, p.y))
            hi = simd_max(hi, SIMD2(p.x, p.y))
        }
        if session.finsOn, let layer = s.supportLayer(device: device), let fins = layer.fins, layer.finVertices > 0 {
            let base = fins.contents().bindMemory(to: Float.self, capacity: layer.finVertices * layer.stride)
            for v in 0..<layer.finVertices {
                let x = base[v * layer.stride], y = base[v * layer.stride + 1]
                lo = simd_min(lo, SIMD2(x, y))
                hi = simd_max(hi, SIMD2(x, y))
            }
        }
        return (Double(lo.x) + m.xy.x, Double(lo.y) + m.xy.y, Double(hi.x) + m.xy.x, Double(hi.y) + m.xy.y)
    }
}
