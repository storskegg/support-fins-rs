import Metal
import SupportFinsCore
import Viewer
import simd

/// A build as the viewer draws it: the Rust allocations the engine returned, wrapped with no copy
/// (`GpuBuffer.metalBuffer`), in print space; the vertex ranges that are removed fins; the offset the part is
/// seated with.
extension BuildModel {
    /// The fin and pad buffers of `latest`, or nil before a build lands.
    func supportLayer(device: MTLDevice) -> SupportLayer? {
        guard let result = latest else { return nil }
        let fins = result.fins(), pad = result.pad()
        return SupportLayer(
            fins: fins.count() > 0 ? fins.metalBuffer(device: device) : nil, finVertices: Int(fins.count()),
            pad: pad.count() > 0 ? pad.metalBuffer(device: device) : nil, padVertices: Int(pad.count()),
            stride: Int(fins.stride()) / 4)
    }

    /// The seating offset of `latest`: the part draws at `rotation * p + offset` (summary `offset`), the same
    /// translation the fins are already placed with.
    var seatingOffset: SIMD3<Float>? {
        summary.map { SIMD3(Float($0.offset.x), Float($0.offset.y), Float($0.offset.z)) }
    }

    /// The fin buffer's vertex ranges of the removed fins this build matches (`matchedRemovals`): what the
    /// renderer leaves out. A fin's `triRanges` may be several (a wall and its tines).
    func hiddenRanges(removed: Set<FinKey>) -> [Range<Int>] {
        guard let records else { return [] }
        let hit = matchedRemovals(of: removed)
        return records.fins.filter { hit[$0.id] != nil }.flatMap { $0.triRanges.map { $0.from..<$0.to } }
    }

    /// The vertex ranges of one fin by record id (a hover or a click picks a fin by its `triRanges`).
    func ranges(ofFin id: Int) -> [Range<Int>] {
        records?.fins.first { $0.id == id }.map { $0.triRanges.map { $0.from..<$0.to } } ?? []
    }

    /// Puts this build on the renderer: the fins and pad, minus `removed`, and the part's seating. The interface
    /// crest has no input yet: the engine does not say which ranges are crest (`records_json` has none), so the
    /// crest mask stays empty.
    func show(on renderer: ViewerRenderer, removed: Set<FinKey> = []) {
        guard let layer = supportLayer(device: renderer.device) else {
            renderer.setSupports(nil)
            return
        }
        renderer.setSupports(layer)
        renderer.setFinMask(hidden: hiddenRanges(removed: removed), crest: [])
    }
}
