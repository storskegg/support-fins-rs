import Foundation
import simd

/// The handle of the section plane (`web/ui/section.js` `grip`): the geometry and hit test of three.js
/// `TransformControls` in local space, size 0.7 -- an arrow along the plane's normal (translate, Z only) and the
/// rings about its X and Y axes plus the outer view-plane ring (rotate, no Z, no middle ball). In units of
/// `s = distance * min(1.9 tan(fov / 2), 7) * size / 4`, constant on screen.
struct SectionGizmo {
    var plane: SectionPlane
    var camera: OrbitCamera

    static let segments = 64
    /// The shaft runs from the centre to 0.5 on the kept side (`lineGeometry2`, a 0.5 cylinder moved up by
    /// 0.25), and a 0.1 cone (`arrowGeometry`) points out from 0.5 to 0.6 at either end.
    static let arrowReach: Float = 0.5
    static let coneLength: Float = 0.1
    /// The pick geometry of the arrow is two cones (`pickerTranslate.Z`, `CylinderGeometry(0.2, 0, 0.6)` at +-0.3),
    /// each from a point at the centre to a radius of 0.2 at 0.6 either way.
    static let arrowPickReach: Float = 0.6
    static let arrowPickRadius: Float = 0.2
    static let axisRadius: Float = 0.5
    static let screenRadius: Float = 0.75
    static let pickTube: Float = 0.1
    /// `AXIS_HIDE_THRESHOLD`: the arrow is not drawn, nor picked, when it points (nearly) at the eye.
    static let axisHide: Float = 0.99

    var center: SIMD3<Float> { plane.position }

    func unit() -> Float {
        let d = simd_length(camera.eye - center)
        return d * min(1.9 * tan(camera.fovY / 2), 7) * SectionTool.gizmoSize / 4
    }

    var eye: SIMD3<Float> { simd_normalize(camera.eye - center) }

    /// The arrow is drawn and grabbable unless it points down the view.
    var showsArrow: Bool { abs(simd_dot(plane.normal, eye)) <= Self.axisHide }

    /// The shaft's two ends: the centre and 0.5 along the normal.
    func shaft() -> [SIMD3<Float>] {
        [center, center + plane.normal * (Self.arrowReach * unit())]
    }

    /// The two cones as (base, tip): from 0.5 to 0.6 along the normal, and the same the other way.
    func cones() -> [(base: SIMD3<Float>, tip: SIMD3<Float>)] {
        let u = unit()
        return [plane.normal, -plane.normal].map { d in
            (center + d * (Self.arrowReach * u), center + d * ((Self.arrowReach + Self.coneLength) * u))
        }
    }

    /// The near half of the ring about local axis `.ringX` or `.ringY` as `segments + 1` world points.
    func arc(_ handle: SectionHandle) -> [SIMD3<Float>] {
        let (u, w): (SIMD3<Float>, SIMD3<Float>) = handle == .ringX ? (plane.axisY, plane.normal) : (plane.normal, plane.axisX)
        let r = Self.axisRadius * unit()
        let toEye = camera.eye - center
        let mid = atan2(simd_dot(toEye, w), simd_dot(toEye, u))
        return (0...Self.segments).map { i in
            let t = mid - .pi / 2 + .pi * Float(i) / Float(Self.segments)
            return center + r * (cos(t) * u + sin(t) * w)
        }
    }

    /// The outer ring, a full circle in the plane facing the camera.
    func circle() -> [SIMD3<Float>] {
        let e = eye
        let up: SIMD3<Float> = abs(e.z) > 0.999 ? SIMD3(0, 1, 0) : SIMD3(0, 0, 1)
        let u = simd_normalize(simd_cross(up, e)), w = simd_cross(e, u)
        let r = Self.screenRadius * unit()
        return (0...Self.segments).map { i in
            let t = 2 * Float.pi * Float(i) / Float(Self.segments)
            return center + r * (cos(t) * u + sin(t) * w)
        }
    }

    func screen(_ p: SIMD3<Float>, width: Float, height: Float) -> SIMD2<Float> {
        let n = camera.project(p)
        return SIMD2((n.x * 0.5 + 0.5) * width, (0.5 - n.y * 0.5) * height)
    }

    /// The handle under a pixel of a `width x height` view (origin top left): the arrow wins where it crosses a
    /// ring (`push` was created first, `web/ui/section.js:84-88`); otherwise the nearest ring within its tube.
    func hit(atPixel p: SIMD2<Float>, width: Float, height: Float) -> SectionHandle? {
        let d = simd_length(camera.eye - center)
        let ppu = unit() * height / (2 * d * tan(camera.fovY / 2))
        if showsArrow {
            let reach = Self.arrowPickReach * unit()
            let a = screen(center - plane.normal * reach, width: width, height: height)
            let b = screen(center + plane.normal * reach, width: width, height: height)
            // the nearest point of the axis on screen, and the cones' radius there (0 at the centre, 0.2 at the ends)
            let ab = b - a, l2 = simd_length_squared(ab)
            let t = l2 > 0 ? Swift.min(Swift.max(simd_dot(p - a, ab) / l2, 0), 1) : 0
            let radius = Self.arrowPickRadius * abs(2 * t - 1)
            if simd_length(p - (a + t * ab)) <= radius * ppu { return .arrow }
        }
        var best: (SectionHandle, Float)?
        func consider(_ h: SectionHandle, _ pts: [SIMD3<Float>]) {
            let s = pts.map { screen($0, width: width, height: height) }
            var dist = Float.infinity
            for i in 0..<(s.count - 1) { dist = min(dist, RotateGizmo.distance(p, from: s[i], to: s[i + 1])) }
            if dist <= Self.pickTube * ppu, dist < (best?.1 ?? .infinity) { best = (h, dist) }
        }
        consider(.ringX, arc(.ringX))
        consider(.ringY, arc(.ringY))
        consider(.screen, circle())
        return best?.0
    }
}
