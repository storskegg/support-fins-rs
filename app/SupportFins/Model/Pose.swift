import Foundation
import simd

/// A part's orientation: the rotation that takes model coordinates to print coordinates (a pure rotation; the
/// engine's `Analysis.offset` seats it on the plate). A value type so a history snapshot is a copy.
struct Pose: Equatable, Hashable, Sendable {
    var rotation: simd_double3x3

    static let identity = Pose(rotation: matrix_identity_double3x3)

    init(rotation: simd_double3x3) { self.rotation = rotation }

    func hash(into hasher: inout Hasher) { columnMajor.forEach { hasher.combine($0) } }

    /// From the engine's `Rot3`: nine numbers, column-major. Nil unless there are nine finite numbers.
    init?(columnMajor m: [Double]) {
        guard m.count == 9, m.allSatisfy(\.isFinite) else { return nil }
        rotation = simd_double3x3(columns: (SIMD3(m[0], m[1], m[2]), SIMD3(m[3], m[4], m[5]), SIMD3(m[6], m[7], m[8])))
    }

    /// The nine numbers `Part::analyze` / `face_flags` / `Builder::build` take (`docs/FFI.md`).
    var columnMajor: [Double] {
        let c = rotation.columns
        return [c.0.x, c.0.y, c.0.z, c.1.x, c.1.y, c.1.z, c.2.x, c.2.y, c.2.z]
    }

    /// The same rotation for the Metal side (single precision).
    var float3x3: simd_float3x3 {
        let c = rotation.columns
        return simd_float3x3(columns: (SIMD3<Float>(c.0), SIMD3<Float>(c.1), SIMD3<Float>(c.2)))
    }

    enum Axis: Int, CaseIterable, Sendable {
        case x, y, z
        var vector: SIMD3<Double> { SIMD3<Double>(x: self == .x ? 1 : 0, y: self == .y ? 1 : 0, z: self == .z ? 1 : 0) }
        var label: String { ["X", "Y", "Z"][rawValue] }
    }

    /// Turn the part about a WORLD axis (`part.quaternion.premultiply(q)`, `ui/pose.js:231`), in degrees.
    func rotated(about axis: Axis, degrees: Double) -> Pose {
        let q = simd_quatd(angle: degrees * .pi / 180, axis: axis.vector)
        return Pose(rotation: simd_double3x3(q) * rotation)
    }

    /// The 90° step of the Rotate popover (`rotate90`, `ui/pose.js:227`).
    func rotated90(about axis: Axis) -> Pose { rotated(about: axis, degrees: 90) }

    /// Lay a face on the plate (`layClick`, `web/ui/pose.js:312-322`): the face's model-frame unit normal goes
    /// through the pose, then the shortest turn takes it to straight down, applied about the world origin
    /// (`part.quaternion.premultiply(layQuat)`). The reseating onto the plate is the engine's `offset`.
    func laidFlat(onFaceNormal n: SIMD3<Double>) -> Pose {
        let faceNormal = rotation * n
        return Pose(rotation: simd_double3x3(Self.quaternionFromUnitVectors(faceNormal, Self.down)) * rotation)
    }

    /// `DOWN` (`web/ui/pose.js:249`).
    static let down = SIMD3<Double>(0, 0, -1)

    /// three.js `Quaternion.setFromUnitVectors` (`vFrom` and `vTo` unit length), operation for operation,
    /// including the half-turn branch for a from-vector pointing straight away from `to`.
    static func quaternionFromUnitVectors(_ from: SIMD3<Double>, _ to: SIMD3<Double>) -> simd_quatd {
        var r = from.x * to.x + from.y * to.y + from.z * to.z + 1
        var x: Double, y: Double, z: Double
        if r < Double.ulpOfOne {
            // vFrom is opposite vTo
            r = 0
            if abs(from.x) > abs(from.z) { (x, y, z) = (-from.y, from.x, 0) } else { (x, y, z) = (0, -from.z, from.y) }
        } else {
            x = from.y * to.z - from.z * to.y
            y = from.z * to.x - from.x * to.z
            z = from.x * to.y - from.y * to.x
        }
        let length = (x * x + y * y + z * z + r * r).squareRoot()
        return simd_quatd(ix: x / length, iy: y / length, iz: z / length, r: r / length)
    }

    /// Is this the same orientation as `other`, to rounding?
    func isSame(as other: Pose, tolerance: Double = 1e-9) -> Bool {
        let a = rotation.columns, b = other.rotation.columns
        return simd_length(a.0 - b.0) < tolerance && simd_length(a.1 - b.1) < tolerance && simd_length(a.2 - b.2) < tolerance
    }

    // MARK: the readout

    /// The three whole-degree angles of the readout `X 0° · Y 0° · Z 0°`.
    struct Euler: Equatable, Hashable, Sendable {
        var x: Int, y: Int, z: Int
        init(_ x: Int, _ y: Int, _ z: Int) { (self.x, self.y, self.z) = (x, y, z) }
        var readout: String { "X \(x)° · Y \(y)° · Z \(z)°" }
    }

    /// `readableEuler` (`web/ui/part.js:430-443`): the XYZ Euler angles for display, in whichever of the two
    /// equivalent solutions reads better ("Y 180" rather than "X -180, Y 0, Z -180"). The alternate is verified
    /// against the rotation, not trusted.
    var euler: Euler {
        let a = Self.eulerXYZ(rotation).map { Self.wrap180($0 * 180 / .pi) }
        let b = [Self.wrap180(a[0] + 180), Self.wrap180(180 - a[1]), Self.wrap180(a[2] + 180)]
        func cost(_ v: [Double]) -> Double { abs(v[0]) + abs(v[1]) + abs(v[2]) }
        if cost(b) < cost(a) {
            let again = Self.fromEulerXYZ(degrees: b)
            if abs(abs(simd_dot(simd_quatd(again), simd_quatd(rotation))) - 1) < 1e-6 {
                return Euler(Self.jsRound(b[0]), Self.jsRound(b[1]), Self.jsRound(b[2]))
            }
        }
        return Euler(Self.jsRound(a[0]), Self.jsRound(a[1]), Self.jsRound(a[2]))
    }

    /// three.js `Euler.setFromRotationMatrix(m, 'XYZ')` (radians).
    static func eulerXYZ(_ m: simd_double3x3) -> [Double] {
        let m13 = m.columns.2.x, m23 = m.columns.2.y, m33 = m.columns.2.z
        let m12 = m.columns.1.x, m11 = m.columns.0.x, m32 = m.columns.1.z, m22 = m.columns.1.y
        let y = asin(min(max(m13, -1), 1))
        if abs(m13) < 0.9999999 { return [atan2(-m23, m33), y, atan2(-m12, m11)] }
        return [atan2(m32, m22), y, 0]
    }

    /// `Rx * Ry * Rz` (three.js Euler order XYZ), degrees.
    static func fromEulerXYZ(degrees d: [Double]) -> simd_double3x3 {
        let r = d.map { $0 * .pi / 180 }
        let rx = simd_double3x3(simd_quatd(angle: r[0], axis: SIMD3(1, 0, 0)))
        let ry = simd_double3x3(simd_quatd(angle: r[1], axis: SIMD3(0, 1, 0)))
        let rz = simd_double3x3(simd_quatd(angle: r[2], axis: SIMD3(0, 0, 1)))
        return rx * ry * rz
    }

    /// `wrap180` (`web/ui/part.js:416-420`): into -180...180 with half a turn as +180, and no negative zero.
    static func wrap180(_ deg: Double) -> Double {
        var v = ((deg + 180).truncatingRemainder(dividingBy: 360) + 360).truncatingRemainder(dividingBy: 360) - 180
        if v == 0 { v = 0 }
        return v == -180 ? 180 : v
    }

    /// `Math.round`: halves go up.
    static func jsRound(_ v: Double) -> Int { Int((v + 0.5).rounded(.down)) }
}
