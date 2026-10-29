import Metal
import SupportFinsCore
import Viewer
import simd

/// The plates on the viewer (`drawRow`, `ui/plateview.js`) and the footprints of the models on them
/// (`slotGrid`, `ui/arrange.js`).
extension PlateBoard {
    /// Lay the scene out for the plate being edited: its bed shifted by minus the live model's spot, and every parked
    /// model where it stands, in the ghost look.
    func sync() {
        guard let controller = host.controller else { return }
        let live = plate.liveSlot
        let shift = SIMD2<Float>(Float(-live.xy.x), Float(-live.xy.y))
        let device = controller.renderer.device
        var parked: [ParkedDrawable] = []
        var beds: [SIMD2<Float>] = []
        parkedSlots = []
        parkedPlates = []
        // the row: plate i stands (i - active) * pitch along X from the one being edited (`drawRow`)
        let p = Float(pitch)
        for (pi, pl) in plates.enumerated() {
            let bed = SIMD2<Float>(Float(pi - active) * p, 0) + shift
            beds.append(bed)
            for (k, m) in pl.models.enumerated() where !(pi == active && k == pl.live) {
                guard let s = m.stash, let flags = s.flags ?? zeroFlags(s.part, device) else { continue }
                // another plate's fins stay as they were left, under its own settings
                let finsOn = pi == active ? session.finsOn : (pl.settings?.finsOn ?? false)
                let layer = finsOn ? s.supportLayer(device: device) : nil
                let plateSettings = pi == active ? session.settings : (pl.settings?.settings ?? session.settings)
                let drawn = host.features[DrawFeature.self]?.parkedDrawn(s, settings: plateSettings, finsOn: finsOn) { [weak self] in self?.sync() }
                parked.append(ParkedDrawable(
                    mesh: s.part, flags: flags, rotation: s.pose.float3x3, offset: s.offset,
                    origin: SIMD2(Float(m.xy.x), Float(m.xy.y)) + bed,
                    supports: layer, hiddenFins: layer == nil ? [] : s.build.hiddenRanges(removed: s.removedFins),
                    drawn: drawn?.mesh, stale: pi == active && isStale(s)))
                parkedSlots.append(m)
                parkedPlates.append(pi)
            }
        }
        controller.renderer.setPlateScene(PlateScene(beds: beds, edited: active, parked: parked))
    }

    private func zeroFlags(_ part: CorePart, _ device: MTLDevice) -> MTLBuffer? {
        device.makeBuffer(length: max(part.faceCount, 1), options: .storageModeShared)
    }

    // MARK: footprints

    /// Slot `m`'s footprint about its own spot (nil for an empty slot): the live one from what the export writes, a
    /// parked one from the fins it shows (`slotGrid`, `ui/arrange.js`).
    func footprint(of m: ModelSlot) -> Footprint? {
        let key: FootprintKey, part: Part, wanted: BuildResult?
        var drawn: DrawnSweep?
        if m === plate.liveSlot, hasLive, let first = session.parts.first {
            let build = host.wiring?.build
            let latest = session.finsOn ? build?.latest : nil
            let ids = latest == nil ? [] : (build?.matchedRemovals(of: session.removedFins).keys.sorted().map { UInt64($0) } ?? [])
            drawn = latest == nil ? nil : host.features[DrawFeature.self]?.model.exportDrawn.sweep
            key = FootprintKey(pose: session.pose.columnMajor, offset: host.controller?.renderer.offset ?? .zero,
                               build: latest.map(ObjectIdentifier.init), removed: ids, drawn: drawn.map(ObjectIdentifier.init))
            part = first.part
            wanted = latest
        } else if let s = m.stash {
            let latest = session.finsOn ? s.build.latest : nil
            let ids = latest == nil ? [] : s.build.matchedRemovals(of: s.removedFins).keys.sorted().map { UInt64($0) }
            drawn = latest == nil ? nil : host.features[DrawFeature.self]?.parkedDrawn(s, settings: session.settings, finsOn: session.finsOn) { [weak self] in self?.sync() }?.sweep
            key = FootprintKey(pose: s.pose.columnMajor, offset: s.offset, build: latest.map(ObjectIdentifier.init), removed: ids,
                               drawn: drawn.map(ObjectIdentifier.init))
            part = s.part.part
            wanted = latest
        } else {
            return nil
        }
        if let c = m.footprintCache, c.key == key { return c.grid }
        let o = key.offset
        let cells = try? footprintOfPart(part: part, rot: key.pose, offset: [Double(o.x), Double(o.y), Double(o.z)])
        let added = wanted.flatMap { footprintOfBuildWithDrawn(build: $0, drawn: drawn, removed: key.removed) }
        let grid = footprintUnion(a: cells, b: added)
        m.footprintCache = (key, grid)
        return grid
    }

    /// The footprints of the parked models of `plate` standing where they stand, but `skipping`: what a new model
    /// must keep `gap` clear of.
    func placedFootprints(skipping: [ModelSlot], in plate: Plate? = nil) -> [Footprint] {
        let p = plate ?? self.plate
        return p.models.compactMap { m in
            guard m.stash != nil, !skipping.contains(where: { $0 === m }) else { return nil }
            return footprint(of: m)?.placed(x: m.xy.x, y: m.xy.y)
        }
    }
}
