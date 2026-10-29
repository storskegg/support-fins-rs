import Foundation
import Metal
import Observation
import simd

/// The Move tool's pointer (`web/ui/move.js`): while it is on, a press that starts on a model (the live one with its
/// fins, or a parked one) drags it across its plate, and a press anywhere else orbits as before. This object finds the
/// model under the pointer (the GPU pick the hover already runs), the height it was grabbed at, and where the pointer
/// is on the plane at that height, so the grabbed point stays under the pointer; what a drag means for the plate (the
/// slot's spot, the clamp against the others, the undo step) is the app's, through `onBegin` / `onDrag` / `onEnd`.
@MainActor
@Observable
public final class MoveTool {
    /// `moving`.
    public private(set) var isOn = false
    /// The model under the pointer (the web's `modelAt` for the `grab` cursor).
    @ObservationIgnored public private(set) var hover: ModelPick?
    /// A model is being dragged.
    @ObservationIgnored public private(set) var dragging = false
    @ObservationIgnored weak var renderer: ViewerRenderer?
    @ObservationIgnored public var onCursorChange: (() -> Void)?
    /// The tool was switched on or off (rings come back, the toolbar lights).
    @ObservationIgnored public var onChange: (() -> Void)?

    /// A press grabbed `pick` at `world` (print-space xy of the pointer on the plane the model was hit at); the app
    /// says whether it takes the drag.
    @ObservationIgnored public var onBegin: ((ModelPick, SIMD2<Double>) -> Bool)?
    /// The pointer moved with the model grabbed: where it is now, as `onBegin` gave it.
    @ObservationIgnored public var onDrag: ((SIMD2<Double>) -> Void)?
    /// The button is up (or the tool was switched off): the drag is over.
    @ObservationIgnored public var onEnd: (() -> Void)?
    /// The M key (`ui/move.js:190-196`): the app toggles the tool.
    @ObservationIgnored public var onToggleKey: (() -> Void)?

    @ObservationIgnored private var plateZ: Float = 0
    @ObservationIgnored private var hoverPoint = false

    public init() {}

    /// `setMove`.
    public func setOn(_ on: Bool) {
        guard on != isOn else { return }
        isOn = on
        if !on { endDrag() ; setHover(nil) }
        renderer?.changeCountBump()
        onChange?()
        onCursorChange?()
    }

    /// The pointer is over `pick` (or nothing): the cursor says so.
    public func setHover(_ pick: ModelPick?) {
        guard pick != hover else { return }
        hover = pick
        onCursorChange?()
    }

    /// Where the pointer is, print-space xy, on the horizontal plane at height `z` (`plateAtPointer`).
    func point(x: Int, y: Int, width: Int, height: Int, z: Float) -> SIMD2<Double>? {
        guard let r = renderer, width > 0, height > 0 else { return nil }
        let ray = r.camera.ray(ndcX: 2 * Float(x) / Float(width) - 1, ndcY: 1 - 2 * Float(y) / Float(height))
        guard abs(ray.direction.z) > 1e-9 else { return nil }
        let t = (z - ray.origin.z) / ray.direction.z
        guard t > 0 else { return nil }
        let p = ray.origin + t * ray.direction
        return SIMD2(Double(p.x), Double(p.y))
    }

    /// A press: starts a drag when it lands on a model the app lets move. True when it did (no orbit, no pick).
    public func begin(x: Int, y: Int, width: Int, height: Int) -> Bool {
        guard isOn, !dragging, let r = renderer, let pick = hover, width > 0, height > 0 else { return false }
        let ray = r.camera.ray(ndcX: 2 * Float(x) / Float(width) - 1, ndcY: 1 - 2 * Float(y) / Float(height))
        // the height the model was hit at: that point stays under the pointer as it slides
        plateZ = r.hitHeight(of: pick, ray: ray) ?? 0
        guard let grab = point(x: x, y: y, width: width, height: height, z: plateZ), onBegin?(pick, grab) == true else { return false }
        dragging = true
        onCursorChange?()
        return true
    }

    /// The pointer moved with the button down.
    public func dragged(x: Int, y: Int, width: Int, height: Int) {
        guard dragging, let p = point(x: x, y: y, width: width, height: height, z: plateZ) else { return }
        onDrag?(p)
    }

    /// The button is up.
    public func ended() { endDrag() }

    private func endDrag() {
        guard dragging else { return }
        dragging = false
        onEnd?()
        onCursorChange?()
    }
}

extension ViewerRenderer {
    func changeCountBump() { changeCount &+= 1 }

    /// The height at which `ray` meets what `pick` is made of -- its part, its fins and its pad and its drawn walls, whichever is nearest
    /// (`modelAt` raycasts `[part, finMesh, padMesh, drawnMesh]`, `ui/move.js:73-76`): the plane the model is dragged
    /// on, so the point grabbed stays under the pointer. One pass over the triangles at the press.
    func hitHeight(of pick: ModelPick, ray: (origin: SIMD3<Float>, direction: SIMD3<Float>)) -> Float? {
        var best = Float.infinity
        func consider(_ buffer: MTLBuffer, stride: Int, vertices: Range<Int>, matrix: simd_float4x4) {
            if let t = Self.nearest(ray, in: buffer, stride: stride, vertices: vertices, matrix: matrix) { best = min(best, t) }
        }
        switch pick {
        case .live:
            if let m = meshes.first { consider(m.vertexBuffer, stride: m.vertexStride / 4, vertices: 0..<(m.faceCount * 3), matrix: modelMatrix) }
            if let s = supports { supportHits(s, hidden: hiddenFinRanges, matrix: matrix_identity_float4x4, consider) }
            if let w = drawScene?.walls { consider(w.buffer, stride: 6, vertices: 0..<w.vertices, matrix: matrix_identity_float4x4) }
        case .parked(let i):
            guard plateScene.parked.indices.contains(i) else { return nil }
            let p = plateScene.parked[i]
            consider(p.mesh.vertexBuffer, stride: p.mesh.vertexStride / 4, vertices: 0..<(p.mesh.faceCount * 3), matrix: p.matrix)
            if let s = p.supports { supportHits(s, hidden: p.hiddenFins, matrix: simd_float4x4(translation: SIMD3(p.origin, 0)), consider) }
            if let d = p.drawn { consider(d.buffer, stride: 6, vertices: 0..<d.vertices, matrix: simd_float4x4(translation: SIMD3(p.origin, 0))) }
        }
        return best.isFinite ? ray.origin.z + best * ray.direction.z : nil
    }

    private func supportHits(_ s: SupportLayer, hidden: [Range<Int>], matrix: simd_float4x4,
                             _ consider: (MTLBuffer, Int, Range<Int>, simd_float4x4) -> Void) {
        if let fins = s.fins {
            for seg in SupportSegments.fins(total: s.finVertices, hidden: hidden, crest: []) { consider(fins, s.stride, seg.range, matrix) }
        }
        if let pad = s.pad, s.padVertices > 0 { consider(pad, s.stride, 0..<s.padVertices, matrix) }
    }

    /// The nearest `t` at which `ray` meets the triangles of `vertices` (three a triangle) of `buffer`, drawn under
    /// `matrix` (a rotation and a translation, so `t` is the same in the model's frame): Moller-Trumbore.
    static func nearest(_ ray: (origin: SIMD3<Float>, direction: SIMD3<Float>), in buffer: MTLBuffer, stride: Int,
                        vertices: Range<Int>, matrix: simd_float4x4) -> Float? {
        let inv = matrix.inverse
        let o4 = inv * SIMD4(ray.origin, 1), d4 = inv * SIMD4(ray.direction, 0)
        let o = SIMD3(o4.x, o4.y, o4.z), d = SIMD3(d4.x, d4.y, d4.z)
        let base = buffer.contents().bindMemory(to: Float.self, capacity: max(vertices.upperBound, 1) * stride)
        func v(_ i: Int) -> SIMD3<Float> { SIMD3(base[i * stride], base[i * stride + 1], base[i * stride + 2]) }
        var best = Float.infinity
        var f = vertices.lowerBound
        while f + 2 < vertices.upperBound {
            let a = v(f), e1 = v(f + 1) - a, e2 = v(f + 2) - a
            f += 3
            let p = simd_cross(d, e2), det = simd_dot(e1, p)
            if abs(det) < 1e-12 { continue }
            let s = o - a, u = simd_dot(s, p) / det
            if u < 0 || u > 1 { continue }
            let q = simd_cross(s, e1), w = simd_dot(d, q) / det
            if w < 0 || u + w > 1 { continue }
            let t = simd_dot(e2, q) / det
            if t > 0 && t < best { best = t }
        }
        return best.isFinite ? best : nil
    }
}
