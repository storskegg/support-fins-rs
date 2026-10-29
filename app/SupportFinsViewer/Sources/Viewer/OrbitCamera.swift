import Foundation
import simd

/// The Z-up orbit camera of the web app (`web/ui/scene.js`, three.js `OrbitControls`): it looks at
/// `target` from `distance` along the direction given by `azimuth` (about Z, from +X, counter-clockwise
/// seen from above) and `elevation` (above the XY plane). World units are millimetres, the plate is z = 0.
public struct OrbitCamera: Equatable, Sendable {
    public var target: SIMD3<Float>
    public var distance: Float
    public var azimuth: Float
    public var elevation: Float
    public var fovY: Float
    public var near: Float
    public var far: Float
    public var aspect: Float

    /// OrbitControls' zoom range is unbounded; these keep the matrices finite.
    public static let minDistance: Float = 1
    public static let maxDistance: Float = 20000
    /// Just short of the poles: straight up has no heading (the web app's views lean `TILT` off it).
    public static let maxElevation: Float = .pi / 2 - 1e-4

    /// The default view's direction (`HOME_DIR`, `web/ui/viewpresets.js`): front right, from above.
    public static let homeDirection = SIMD3<Float>(0.62, -0.72, 0.55)

    public init(target: SIMD3<Float> = .zero, distance: Float = 300, azimuth: Float = 0, elevation: Float = 0.5,
                fovY: Float = 45 * .pi / 180, near: Float = 1, far: Float = 5000, aspect: Float = 1) {
        self.target = target
        self.distance = distance
        self.azimuth = azimuth
        self.elevation = elevation
        self.fovY = fovY
        self.near = near
        self.far = far
        self.aspect = aspect
    }

    /// The camera looking back along `HOME_DIR` at the origin, from `distance` away.
    public static func home(distance: Float = 300, aspect: Float = 1) -> OrbitCamera {
        var c = OrbitCamera(distance: distance, aspect: aspect)
        c.look(alongFromTarget: homeDirection)
        return c
    }

    /// Unit vector from the target to the camera.
    public var direction: SIMD3<Float> {
        SIMD3(cos(elevation) * cos(azimuth), cos(elevation) * sin(azimuth), sin(elevation))
    }

    public var eye: SIMD3<Float> { target + direction * distance }

    /// Set azimuth and elevation so the camera sits along `dir` (any length) from the target.
    public mutating func look(alongFromTarget dir: SIMD3<Float>) {
        let n = simd_normalize(dir)
        azimuth = atan2(n.y, n.x)
        elevation = min(max(asin(n.z), -Self.maxElevation), Self.maxElevation)
    }

    /// Camera axes: right, up (screen), and forward (into the scene).
    public var basis: (right: SIMD3<Float>, up: SIMD3<Float>, forward: SIMD3<Float>) {
        let forward = -direction
        let right = simd_normalize(simd_cross(forward, SIMD3(0, 0, 1)))
        let up = simd_cross(right, forward)
        return (right, up, forward)
    }

    // MARK: matrices

    public var viewMatrix: simd_float4x4 {
        let b = basis
        let e = eye
        return simd_float4x4(columns: (
            SIMD4(b.right.x, b.up.x, -b.forward.x, 0),
            SIMD4(b.right.y, b.up.y, -b.forward.y, 0),
            SIMD4(b.right.z, b.up.z, -b.forward.z, 0),
            SIMD4(-simd_dot(b.right, e), -simd_dot(b.up, e), simd_dot(b.forward, e), 1)))
    }

    /// Right-handed perspective with Metal's 0...1 clip depth.
    public var projectionMatrix: simd_float4x4 {
        let f = 1 / tan(fovY / 2)
        let z = far / (near - far)
        return simd_float4x4(columns: (
            SIMD4(f / aspect, 0, 0, 0),
            SIMD4(0, f, 0, 0),
            SIMD4(0, 0, z, -1),
            SIMD4(0, 0, z * near, 0)))
    }

    public var viewProjection: simd_float4x4 { projectionMatrix * viewMatrix }

    // MARK: navigation (the OrbitControls maths)

    /// A drag of (`dx`, `dy`) points in a view `viewHeight` points tall orbits by 2 pi per view height
    /// (`rotateLeft`/`rotateUp` of OrbitControls): dragging right turns the scene right.
    public mutating func orbit(dx: Float, dy: Float, viewHeight: Float) {
        guard viewHeight > 0 else { return }
        azimuth -= 2 * .pi * dx / viewHeight
        elevation = min(max(elevation + 2 * .pi * dy / viewHeight, -Self.maxElevation), Self.maxElevation)
    }

    /// Pan in the screen plane so the point under the pointer follows it (screen-space panning).
    public mutating func pan(dx: Float, dy: Float, viewHeight: Float) {
        guard viewHeight > 0 else { return }
        let b = basis
        let k = 2 * distance * tan(fovY / 2) / viewHeight
        target += (-b.right * dx + b.up * dy) * k
    }

    /// Multiply the distance by `1 / factor` (a factor above 1 moves in).
    public mutating func zoom(by factor: Float) {
        guard factor > 0 else { return }
        distance = min(max(distance / factor, Self.minDistance), Self.maxDistance)
    }

    /// Zoom by `factor` keeping the point under the pointer (`ndc`, -1...1, y up) where it is: the point of the plane
    /// through the target, facing the camera, that the pointer is over stays under it, so a pinch zooms toward the
    /// cursor (native; the web's `zoomToCursor` is off).
    public mutating func zoom(by factor: Float, towardNDC ndc: SIMD2<Float>) {
        guard factor > 0, let before = focusPoint(ndc) else { return zoom(by: factor) }
        zoom(by: factor)
        if let after = focusPoint(ndc) { target += before - after }
    }

    /// Where the pointer's ray meets the plane through the target that faces the camera.
    private func focusPoint(_ ndc: SIMD2<Float>) -> SIMD3<Float>? {
        let r = ray(ndcX: ndc.x, ndcY: ndc.y)
        let f = basis.forward
        let denom = simd_dot(r.direction, f)
        guard abs(denom) > 1e-6 else { return nil }
        return r.origin + r.direction * (simd_dot(target - r.origin, f) / denom)
    }

    /// A mouse-wheel tick: `getZoomScale` of OrbitControls, `0.95 ^ (|delta| * 0.01)` per unit.
    public mutating func zoom(wheelDelta delta: Float) {
        let scale = pow(0.95, abs(delta * 0.01))
        zoom(by: delta < 0 ? 1 / scale : scale)   // scroll up moves in
    }

    /// Point the camera at the part, backed off far enough to see all of it (`frame` in `web/ui/scene.js`):
    /// the home direction, `reach * 2.1` away (more for a portrait view), looking at the middle of the height.
    public mutating func frame(size: SIMD3<Float>) {
        let reach = max(size.x, size.y, size.z, 40)
        distance = reach * 2.1 * max(1, 0.7 / aspect)
        look(alongFromTarget: Self.homeDirection)
        near = max(0.5, reach / 200)
        far = distance * 12
        target = SIMD3(0, 0, size.z / 2)
    }

    // MARK: picking rays

    /// The world ray through a point of the view, in normalised device coordinates (-1...1, y up).
    public func ray(ndcX: Float, ndcY: Float) -> (origin: SIMD3<Float>, direction: SIMD3<Float>) {
        let inv = viewProjection.inverse
        let a = inv * SIMD4(ndcX, ndcY, 0, 1)
        let b = inv * SIMD4(ndcX, ndcY, 1, 1)
        let p0 = SIMD3(a.x, a.y, a.z) / a.w
        let p1 = SIMD3(b.x, b.y, b.z) / b.w
        return (p0, simd_normalize(p1 - p0))
    }

    /// Project a world point to normalised device coordinates (x, y in -1...1, z clip depth 0...1).
    public func project(_ p: SIMD3<Float>) -> SIMD3<Float> {
        let c = viewProjection * SIMD4(p.x, p.y, p.z, 1)
        return SIMD3(c.x, c.y, c.z) / c.w
    }
}
