import Foundation
import simd

public enum GizmoAxis: Int, CaseIterable, Sendable {
    case x, y, z

    public var vector: SIMD3<Float> { [SIMD3(1, 0, 0), SIMD3(0, 1, 0), SIMD3(0, 0, 1)][rawValue] }
    public var label: String { ["X", "Y", "Z"][rawValue] }
    /// three.js `TransformControls`' ring colours (`0xff0000`, `0x00ff00`, `0x0000ff`).
    public var color: SIMD3<Float> { vector }
    /// Two unit vectors spanning the plane the ring lies in (right-handed with the axis: u x w = axis).
    var basis: (u: SIMD3<Float>, w: SIMD3<Float>) {
        switch self {
        case .x: (SIMD3(0, 1, 0), SIMD3(0, 0, 1))
        case .y: (SIMD3(0, 0, 1), SIMD3(1, 0, 0))
        case .z: (SIMD3(1, 0, 0), SIMD3(0, 1, 0))
        }
    }
}

/// The rotate rings of the web app (`web/ui/pose.js`: three.js `TransformControls` in rotate mode, size 0.85,
/// a 5 degree snap that Shift releases), drawn by the viewer around the part. Pure maths here: the ring
/// geometry, its hit test and the drag; `ViewerRenderer` draws it and `ViewerController` feeds it the pointer.
///
/// Like three.js, each ring is the half circle that faces the camera, the ring is a constant size on screen,
/// and the rings are world-aligned. Unlike it, a drag follows the pointer's angle around the centre, which is
/// what "dragging the ring" means to a hand.
public struct RotateGizmo: Sendable {
    public static let size: Float = 0.85
    public static let snapDegrees: Double = 5
    /// How close, in pixels, the pointer must be to a ring to grab it.
    public static let pickPixels: Float = 9
    public static let segments = 64

    public var center: SIMD3<Float>

    public init(center: SIMD3<Float>) { self.center = center }

    /// World radius of the rings: `factor * size / 4` with `factor = distance * min(1.9 tan(fov / 2), 7)`
    /// (`TransformControlsGizmo.updateMatrixWorld`), so a ring keeps its size on screen.
    public func radius(camera: OrbitCamera) -> Float {
        let d = simd_length(camera.eye - center)
        return d * min(1.9 * tan(camera.fovY / 2), 7) * Self.size / 4
    }

    /// The near half of one ring as a polyline of `segments + 1` world points.
    public func arc(_ axis: GizmoAxis, camera: OrbitCamera) -> [SIMD3<Float>] {
        let r = radius(camera: camera)
        let (u, w) = axis.basis
        let toEye = camera.eye - center
        // where the camera is, in the ring's own angle; the visible half spans a quarter turn either side
        let mid = atan2(simd_dot(toEye, w), simd_dot(toEye, u))
        return (0...Self.segments).map { i in
            let t = mid - .pi / 2 + .pi * Float(i) / Float(Self.segments)
            return center + r * (cos(t) * u + sin(t) * w)
        }
    }

    /// The ring under a pixel of a `width x height` view (origin top left), if any: the nearest within
    /// `pickPixels`.
    public func hit(axes: [GizmoAxis] = GizmoAxis.allCases, atPixel p: SIMD2<Float>, camera: OrbitCamera,
                    width: Float, height: Float) -> GizmoAxis? {
        var best: (GizmoAxis, Float)?
        for axis in axes {
            let pts = arc(axis, camera: camera).map { screen($0, camera: camera, width: width, height: height) }
            var d = Float.infinity
            for i in 0..<(pts.count - 1) { d = min(d, Self.distance(p, from: pts[i], to: pts[i + 1])) }
            if d <= Self.pickPixels, d < (best?.1 ?? .infinity) { best = (axis, d) }
        }
        return best?.0
    }

    func screen(_ p: SIMD3<Float>, camera: OrbitCamera, width: Float, height: Float) -> SIMD2<Float> {
        let n = camera.project(p)
        return SIMD2((n.x * 0.5 + 0.5) * width, (0.5 - n.y * 0.5) * height)
    }

    static func distance(_ p: SIMD2<Float>, from a: SIMD2<Float>, to b: SIMD2<Float>) -> Float {
        let ab = b - a, l2 = simd_length_squared(ab)
        let t = l2 > 0 ? min(max(simd_dot(p - a, ab) / l2, 0), 1) : 0
        return simd_length(p - (a + t * ab))
    }

    // MARK: dragging

    /// A drag in progress: the vector from the centre to where the ray met the ring's plane at the start.
    public struct Drag: Sendable {
        public let axis: GizmoAxis
        let start: SIMD3<Float>
    }

    /// Where a view ray meets the plane through the centre perpendicular to `axis`, as a vector from the centre.
    func planePoint(axis: GizmoAxis, ray: (origin: SIMD3<Float>, direction: SIMD3<Float>)) -> SIMD3<Float>? {
        let n = axis.vector
        let denom = simd_dot(ray.direction, n)
        guard abs(denom) > 1e-3 else { return nil }          // the plane is edge-on: no stable point
        let t = simd_dot(center - ray.origin, n) / denom
        guard t > 0 else { return nil }
        return ray.origin + t * ray.direction - center
    }

    public func beginDrag(_ axis: GizmoAxis, ray: (origin: SIMD3<Float>, direction: SIMD3<Float>)) -> Drag? {
        planePoint(axis: axis, ray: ray).map { Drag(axis: axis, start: $0) }
    }

    /// The turn about the ring's axis since the drag began, in degrees: the pointer's angle round the centre,
    /// snapped to 5 degrees unless `snap` is false (Shift). Nil while the ray cannot meet the plane.
    public func angle(_ drag: Drag, ray: (origin: SIMD3<Float>, direction: SIMD3<Float>), snap: Bool) -> Double? {
        guard let now = planePoint(axis: drag.axis, ray: ray) else { return nil }
        let a = drag.axis.vector
        let radians = Double(atan2(simd_dot(a, simd_cross(drag.start, now)), simd_dot(drag.start, now)))
        let degrees = radians * 180 / .pi
        return snap ? (degrees / Self.snapDegrees).rounded() * Self.snapDegrees : degrees
    }

    /// The rotation of `degrees` about the world axis (through the centre).
    public static func rotation(_ axis: GizmoAxis, degrees: Double) -> simd_double3x3 {
        simd_double3x3(simd_quatd(angle: degrees * .pi / 180, axis: SIMD3<Double>(SIMD3<Float>(axis.vector))))
    }
}
