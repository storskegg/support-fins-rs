import Foundation
import simd

/// An axis-aligned box in print space (`THREE.Box3`).
public struct SectionBox: Equatable, Sendable {
    public var min: SIMD3<Float>
    public var max: SIMD3<Float>
    public init(min: SIMD3<Float>, max: SIMD3<Float>) { self.min = min; self.max = max }
    public var center: SIMD3<Float> { (min + max) / 2 }
    public var size: SIMD3<Float> { max - min }

    /// `Box3.setFromObject` of a part whose geometry has `bounds` in the model frame, under `model`: the box of
    /// the eight transformed corners.
    public static func world(of bounds: (min: SIMD3<Float>, max: SIMD3<Float>), model: simd_float4x4) -> SectionBox {
        var lo = SIMD3<Float>(repeating: .infinity), hi = SIMD3<Float>(repeating: -.infinity)
        for i in 0..<8 {
            let c = SIMD3<Float>(i & 1 == 0 ? bounds.min.x : bounds.max.x, i & 2 == 0 ? bounds.min.y : bounds.max.y,
                                 i & 4 == 0 ? bounds.min.z : bounds.max.z)
            let w = model * SIMD4(c, 1)
            lo = simd_min(lo, SIMD3(w.x, w.y, w.z))
            hi = simd_max(hi, SIMD3(w.x, w.y, w.z))
        }
        return SectionBox(min: lo, max: hi)
    }
}

/// The section plane's handle (`handle` in `web/ui/section.js`) and the clipping plane read from it: a position
/// and an orientation in print space, whose local +Z is the KEPT side (three.js discards where the signed
/// distance is below 0). Pure maths, so the pose, the aim, the push and the flip can be tested without a GPU.
public struct SectionPlane: Equatable, Sendable {
    public var position: SIMD3<Float>
    public var orientation: simd_quatf

    public init(position: SIMD3<Float> = .zero, orientation: simd_quatf = simd_quatf(ix: 0, iy: 0, iz: 0, r: 1)) {
        self.position = position
        self.orientation = orientation
    }

    public var axisX: SIMD3<Float> { orientation.act(SIMD3(1, 0, 0)) }
    public var axisY: SIMD3<Float> { orientation.act(SIMD3(0, 1, 0)) }
    /// The plane's normal: the kept side (`syncPlane`).
    public var normal: SIMD3<Float> { orientation.act(SIMD3(0, 0, 1)) }

    /// `plane.setFromNormalAndCoplanarPoint`: xyz the normal, w the constant, so `distance(p) = n . p + w`.
    public var clip: SIMD4<Float> {
        let n = normal
        return SIMD4(n, -simd_dot(n, position))
    }

    /// `plane.distanceToPoint`; a point is kept where this is not negative (`keptHit`).
    public func distance(to p: SIMD3<Float>) -> Float { simd_dot(normal, p) + clip.w }

    /// The handle's matrix (rotation and position, no scale).
    public var matrix: simd_float4x4 {
        let m = simd_float3x3(orientation)
        return simd_float4x4(columns: (SIMD4(m.columns.0, 0), SIMD4(m.columns.1, 0), SIMD4(m.columns.2, 0), SIMD4(position, 1)))
    }

    public init(matrix m: simd_float4x4) {
        position = SIMD3(m.columns.3.x, m.columns.3.y, m.columns.3.z)
        let x = simd_normalize(SIMD3(m.columns.0.x, m.columns.0.y, m.columns.0.z))
        let y = simd_normalize(SIMD3(m.columns.1.x, m.columns.1.y, m.columns.1.z))
        let z = simd_normalize(SIMD3(m.columns.2.x, m.columns.2.y, m.columns.2.z))
        orientation = simd_quatf(simd_float3x3(columns: (x, y, z)))
    }

    /// "Flip" (`section-flip`, `handle.rotateX(Math.PI)`): the same plane, the other side kept.
    public mutating func flip() {
        orientation = orientation * simd_quatf(ix: 1, iy: 0, iz: 0, r: 0)       // a half turn about local X
    }

    /// `handle.translateZ(d)`: along the normal, in mm.
    public mutating func push(_ d: Float) { position += normal * d }

    /// `Quaternion.setFromUnitVectors` (three.js), including its choice of axis for opposite vectors.
    public static func rotation(from a: SIMD3<Float>, to b: SIMD3<Float>) -> simd_quatf {
        var r = simd_dot(a, b) + 1
        var q: SIMD4<Float>
        if r < 1e-8 {     // three.js' epsilon (`Quaternion.setFromUnitVectors`, three r185)
            r = 0
            q = abs(a.x) > abs(a.z) ? SIMD4(-a.y, a.x, 0, r) : SIMD4(0, -a.z, a.y, r)
        } else {
            let c = simd_cross(a, b)
            q = SIMD4(c.x, c.y, c.z, r)
        }
        let n = simd_length(q)
        q /= n
        return simd_quatf(ix: q.x, iy: q.y, iz: q.z, r: q.w)
    }

    /// `aimAtView`: through the part's centre, square to whichever of the PART's own axes points most nearly where
    /// the camera looks, keeping the far half. (Bores in a CAD part run along its axes, so looking at the side of
    /// one and switching the section on opens it lengthwise.) `model` takes the part's frame to print space.
    public static func aimed(look: SIMD3<Float>, model: simd_float4x4, box: SectionBox) -> SectionPlane {
        var best = SIMD3<Float>(0, 0, 1)
        var most: Float = -1
        for i in 0..<3 {
            let c = model[i]
            let a = simd_normalize(SIMD3(c.x, c.y, c.z))
            let d = simd_dot(a, look)
            if abs(d) > most { most = abs(d); best = d < 0 ? -a : a }
        }
        return SectionPlane(position: box.center, orientation: rotation(from: SIMD3(0, 0, 1), to: best))
    }

    /// The sheet's side: the part's box diagonal and a fifth (`sizeSheet`).
    public static func sheetSize(for box: SectionBox) -> Float { simd_length(box.size) * 1.2 }

    /// The most the drag of the sheet may push the cut from where it started, so that it stays within the part's
    /// reach (near the drag plane's horizon `t` runs off): `clamp(t, -reach - off, reach - off)`.
    public static func clampedPush(_ t: Float, start: SIMD3<Float>, normal n: SIMD3<Float>, box: SectionBox) -> Float {
        let reach = simd_length(box.size) / 2
        let off = simd_dot(start - box.center, n)
        return Swift.max(-reach - off, Swift.min(reach - off, t))
    }
}
