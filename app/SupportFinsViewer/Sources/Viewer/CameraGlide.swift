import Foundation
import simd

/// The camera glide of the View buttons and the view cube (`web/ui/camviews.js:36`, `orbitLerp` in
/// `web/ui/viewpresets.js`): the camera swings round the target to look back along a direction, keeping the
/// target and the distance, in 450 ms of ease-in-out.
public enum CameraGlide {
    public static let seconds = 0.45                    // EASE_MS

    /// Ease-in-out quad of the fraction `k` (0...1).
    public static func ease(_ k: Float) -> Float {
        k < 0.5 ? 2 * k * k : 1 - pow(-2 * k + 2, 2) / 2
    }

    /// The camera offset from the target eased `e` of the way from `from` to `to`, swung around the target
    /// rather than straight across: heading and tilt (Z up) turn the short way round, the distance eases on
    /// its own (`orbitLerp`).
    public static func orbitLerp(_ from: SIMD3<Float>, _ to: SIMD3<Float>, _ e: Float) -> SIMD3<Float> {
        // in Double like the web: near the poles (Top and Bottom lean 1e-4 off them) Float loses the heading
        func sph(_ p: SIMD3<Float>) -> (r: Double, h: Double, t: Double) {
            let x = Double(p.x), y = Double(p.y), z = Double(p.z)
            let r = (x * x + y * y + z * z).squareRoot()
            return (r, atan2(y, x), acos(max(-1, min(1, z / r))))
        }
        let a = sph(from), b = sph(to), e = Double(e)
        var dh = (b.h - a.h).truncatingRemainder(dividingBy: 2 * .pi)
        if dh > .pi { dh -= 2 * .pi }
        if dh < -.pi { dh += 2 * .pi }
        let r = a.r + (b.r - a.r) * e, h = a.h + dh * e, t = a.t + (b.t - a.t) * e
        return SIMD3(Float(r * sin(t) * cos(h)), Float(r * sin(t) * sin(h)), Float(r * cos(t)))
    }

    /// Where the camera is `seconds` into a glide from `from` towards looking back along `dir`.
    public static func offset(from: SIMD3<Float>, toward dir: SIMD3<Float>, after elapsed: Double) -> SIMD3<Float> {
        let to = OrbitCamera.position(alongFromTarget: dir, target: .zero, distance: simd_length(from))
        return orbitLerp(from, to, ease(Float(min(1, max(0, elapsed / seconds)))))
    }
}

extension OrbitCamera {
    /// Put the camera at `offset` from the target.
    public mutating func setOffset(_ offset: SIMD3<Float>) {
        let r = simd_length(offset)
        guard r > 0 else { return }
        distance = min(max(r, Self.minDistance), Self.maxDistance)
        look(alongFromTarget: offset)
    }

    /// The camera's offset from its target.
    public var offset: SIMD3<Float> { eye - target }
}
