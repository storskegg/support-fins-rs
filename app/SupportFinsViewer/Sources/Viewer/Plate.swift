import simd

/// The plate: a grid on z = 0, the bed outline and the build-volume box (`buildPlate`, `web/ui/scene.js`).
/// Pure data, so a test can pin it: each list is line segments, two points apiece.
public struct PlateGeometry: Sendable {
    public var minor: [SIMD3<Float>] = []
    public var major: [SIMD3<Float>] = []
    public var edge: [SIMD3<Float>] = []
    public var box: [SIMD3<Float>] = []

    public init(volume v: PrinterVolume) {
        let hx = v.x / 2, hy = v.y / 2, step: Float = 10
        // grid lines on z = 0, brighter every 50 mm
        var x = -(hx / step).rounded(.down) * step
        while x <= hx {
            if abs(x).truncatingRemainder(dividingBy: 50) == 0 { major += [[x, -hy, 0], [x, hy, 0]] } else { minor += [[x, -hy, 0], [x, hy, 0]] }
            x += step
        }
        var y = -(hy / step).rounded(.down) * step
        while y <= hy {
            if abs(y).truncatingRemainder(dividingBy: 50) == 0 { major += [[-hx, y, 0], [hx, y, 0]] } else { minor += [[-hx, y, 0], [hx, y, 0]] }
            y += step
        }
        let c: [SIMD3<Float>] = [[-hx, -hy, 0], [hx, -hy, 0], [hx, hy, 0], [-hx, hy, 0]]
        edge = [c[0], c[1], c[1], c[2], c[2], c[3], c[3], c[0]]
        let t = c.map { SIMD3<Float>($0.x, $0.y, v.z) }
        for i in 0..<4 {
            box += [c[i], c[(i + 1) % 4], t[i], t[(i + 1) % 4], c[i], t[i]]
        }
    }
}
