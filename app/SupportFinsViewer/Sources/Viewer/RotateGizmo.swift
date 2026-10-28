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

/// What the pointer grabbed: one of the three axis rings, the outer screen-space ring (`E`), or the middle
/// (`XYZE`, the free trackball) -- three.js' handle names.
public enum GizmoHandle: Equatable, Hashable, Sendable {
    case axis(GizmoAxis)
    case screen
    case free

    /// The name `TransformControls` gives it, as `pose.js:showDelta` reads it.
    public var name: String {
        switch self {
        case .axis(let a): a.label
        case .screen: "E"
        case .free: "XYZE"
        }
    }

    /// The label of the Rotate popover's delta (`ui/pose.js:109-114`): the axis, or `free` for E and XYZE.
    public var deltaLabel: String {
        if case .axis(let a) = self { return a.label }
        return "free"
    }

    /// Handles that light up with it (`TransformControlsGizmo.updateMatrixWorld`): a handle is lit when its name
    /// is the axis or one of its letters, so grabbing the middle (`XYZE`) lights X, Y, Z and E as well.
    func lights(_ other: GizmoHandle) -> Bool {
        if self == other { return true }
        return self == .free
    }
}

/// The rotate rings of the web app (`web/ui/pose.js`): three.js `TransformControls` in rotate mode, size 0.85,
/// a 5 degree snap that Shift releases, drawn by the viewer around the part. Pure maths here: the geometry, the
/// hit test and the drag; `ViewerRenderer` draws it and `ViewerController` feeds it the pointer.
///
/// The model is three.js': in units of `s = distance * min(1.9 tan(fov / 2), 7) * size / 4` (constant on screen)
/// the axis rings are the camera-facing half circles of radius 0.5, the outer ring (`E`) a full circle of 0.75 in
/// the camera's plane, the middle a gray circle of 0.5 grabbed within 0.25. A drag moves a point on the plane
/// through the centre facing the camera; an axis ring turns by the pointer's displacement along the tangent
/// (`axis x eye`) times `20 / distance`, the middle turns about `eye x displacement` by the displacement's
/// length times that speed (a trackball), and the outer ring turns about the eye by the angle the pointer sweeps
/// round the centre. The 5 degree snap rounds the final angle of every one.
public struct RotateGizmo: Sendable {
    public static let size: Float = 0.85
    public static let snapDegrees: Double = 5
    public static let segments = 64
    /// Radii in units of `s` (`gizmoRotate` in TransformControls.js).
    public static let axisRadius: Float = 0.5
    public static let screenRadius: Float = 0.75
    public static let freeRadius: Float = 0.5
    public static let freePick: Float = 0.25
    /// The pick geometry is a torus of tube 0.1 around each ring.
    public static let pickTube: Float = 0.1

    public var center: SIMD3<Float>

    public init(center: SIMD3<Float>) { self.center = center }

    /// `s`, the world size of one gizmo unit.
    public func unit(camera: OrbitCamera) -> Float {
        let d = simd_length(camera.eye - center)
        return d * min(1.9 * tan(camera.fovY / 2), 7) * Self.size / 4
    }

    /// World radius of an axis ring.
    public func radius(camera: OrbitCamera) -> Float { unit(camera: camera) * Self.axisRadius }

    /// Unit vector from the centre to the camera (`TransformControls.eye`).
    func eye(camera: OrbitCamera) -> SIMD3<Float> { simd_normalize(camera.eye - center) }

    /// The near half of one axis ring as a polyline of `segments + 1` world points.
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

    /// A full circle of `radius` (in units of `s`) in the plane facing the camera: the `E` ring and the gray
    /// trackball outline.
    public func circle(radius: Float, camera: OrbitCamera) -> [SIMD3<Float>] {
        let e = eye(camera: camera)
        let up: SIMD3<Float> = abs(e.z) > 0.999 ? SIMD3(0, 1, 0) : SIMD3(0, 0, 1)
        let u = simd_normalize(simd_cross(up, e)), w = simd_cross(e, u)
        let r = radius * unit(camera: camera)
        return (0...Self.segments).map { i in
            let t = 2 * Float.pi * Float(i) / Float(Self.segments)
            return center + r * (cos(t) * u + sin(t) * w)
        }
    }

    // MARK: picking

    /// Pixels per gizmo unit at the centre in a view `height` pixels tall.
    func pixelsPerUnit(camera: OrbitCamera, height: Float) -> Float {
        let d = simd_length(camera.eye - center)
        return unit(camera: camera) * height / (2 * d * tan(camera.fovY / 2))
    }

    /// The handle under a pixel of a `width x height` view (origin top left), if any. The middle wins inside its
    /// 0.25 sphere; otherwise the ring nearest the pointer within the pick torus (a tube of 0.1 gizmo units).
    public func hit(atPixel p: SIMD2<Float>, camera: OrbitCamera, width: Float, height: Float) -> GizmoHandle? {
        let ppu = pixelsPerUnit(camera: camera, height: height)
        let tolerance = Self.pickTube * ppu
        let c = screen(center, camera: camera, width: width, height: height)
        if simd_length(p - c) <= Self.freePick * ppu { return .free }
        var best: (GizmoHandle, Float)?
        func consider(_ h: GizmoHandle, _ pts: [SIMD3<Float>]) {
            let s = pts.map { screen($0, camera: camera, width: width, height: height) }
            var d = Float.infinity
            for i in 0..<(s.count - 1) { d = min(d, Self.distance(p, from: s[i], to: s[i + 1])) }
            if d <= tolerance, d < (best?.1 ?? .infinity) { best = (h, d) }
        }
        for axis in GizmoAxis.allCases { consider(.axis(axis), arc(axis, camera: camera)) }
        consider(.screen, circle(radius: Self.screenRadius, camera: camera))
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

    /// Whether the axis helper line is drawn for an axis hovered or dragged: not when the axis points nearly
    /// along the view (`AXIS_HIDE`: more than 0.9 of the eye, `TransformControlsGizmo`).
    public func showsAxisLine(_ axis: GizmoAxis, camera: OrbitCamera) -> Bool {
        abs(simd_dot(axis.vector, eye(camera: camera))) <= 0.9
    }

    // MARK: dragging

    /// A drag in progress: the point where the ray met the camera-facing plane through the centre, relative to
    /// the centre (`pointStart`), and the eye at that moment.
    public struct Drag: Sendable {
        public let handle: GizmoHandle
        let start: SIMD3<Float>
        let eye: SIMD3<Float>
        let speed: Float
    }

    func planePoint(eye: SIMD3<Float>, ray: (origin: SIMD3<Float>, direction: SIMD3<Float>)) -> SIMD3<Float>? {
        let denom = simd_dot(ray.direction, eye)
        guard abs(denom) > 1e-6 else { return nil }
        let t = simd_dot(center - ray.origin, eye) / denom
        guard t > 0 else { return nil }
        return ray.origin + t * ray.direction - center
    }

    public func beginDrag(_ handle: GizmoHandle, camera: OrbitCamera,
                          ray: (origin: SIMD3<Float>, direction: SIMD3<Float>)) -> Drag? {
        let e = eye(camera: camera)
        guard let p = planePoint(eye: e, ray: ray) else { return nil }
        // `ROTATION_SPEED = 20 / distance(camera, centre)`
        return Drag(handle: handle, start: p, eye: e, speed: 20 / simd_length(camera.eye - center))
    }

    /// The turn since the drag began: a world axis (through the centre) and an angle in radians, snapped to 5
    /// degrees unless `snap` is false (Shift). Nil while the ray cannot meet the plane.
    public func turn(_ drag: Drag, ray: (origin: SIMD3<Float>, direction: SIMD3<Float>), snap: Bool)
        -> (axis: SIMD3<Float>, radians: Double)? {
        guard let end = planePoint(eye: drag.eye, ray: ray) else { return nil }
        let offset = end - drag.start
        let e = drag.eye
        var axis = e
        var angle: Float = 0
        func inPlane() {
            axis = e
            let a = simd_normalize(drag.start), b = simd_normalize(end)
            angle = acos(min(max(simd_dot(a, b), -1), 1))          // `pointEnd.angleTo(pointStart)`
            angle *= simd_dot(simd_cross(b, a), e) < 0 ? 1 : -1
        }
        switch drag.handle {
        case .free:
            // `rotationAxis = offset x eye`; `angle = offset . (rotationAxis x eye) * speed`
            let cross = simd_cross(offset, e)
            guard simd_length(cross) > 1e-9 else { return (e, 0) }
            axis = simd_normalize(cross)
            angle = simd_dot(offset, simd_cross(axis, e)) * drag.speed
        case .axis(let a):
            let tangent = simd_cross(a.vector, e)
            if simd_length(tangent) < 1e-3 {   // three.js tests for exactly 0; the orbit never gets there
                inPlane()                                  // looking straight down the axis
            } else {
                axis = a.vector
                angle = simd_dot(offset, simd_normalize(tangent)) * drag.speed
            }
        case .screen:
            inPlane()
        }
        var radians = Double(angle)
        if snap {
            let step = Self.snapDegrees * .pi / 180
            radians = (radians / step).rounded() * step
        }
        return (axis, radians)
    }

    /// The rotation of `radians` about a world axis through the centre.
    public static func rotation(axis: SIMD3<Float>, radians: Double) -> simd_double3x3 {
        simd_double3x3(simd_quatd(angle: radians, axis: simd_normalize(SIMD3<Double>(axis))))
    }
}
