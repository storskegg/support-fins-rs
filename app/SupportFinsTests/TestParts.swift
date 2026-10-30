import Foundation
import SupportFinsCore
import Testing

@testable import Support_Fins

/// Binary STLs of boxes, written to the test host's temp directory (it is sandboxed: the upstream checkout cannot
/// be read here) and loaded by the real `load_parts`.
enum TestParts {
    static func box(_ x0: Float, _ x1: Float, _ y0: Float, _ y1: Float, _ z0: Float, _ z1: Float) -> [[Float]] {
        let v: [[Float]] = [
            [x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0],
            [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1],
        ]
        let faces = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]]
        return faces.map { f in f.flatMap { v[$0] } }
    }

    /// `tris` (nine floats each) as a binary STL: no normals, no attributes.
    static func stl(_ tris: [[Float]]) -> Data {
        var data = Data(count: 80)
        var count = UInt32(tris.count)
        withUnsafeBytes(of: &count) { data.append(contentsOf: $0) }
        for t in tris {
            for f in [Float](repeating: 0, count: 3) + t { var x = f; withUnsafeBytes(of: &x) { data.append(contentsOf: $0) } }
            data.append(contentsOf: [0, 0])
        }
        return data
    }

    /// `tris` as a binary STL at `url`; its path.
    @discardableResult
    static func write(_ tris: [[Float]], to url: URL) throws -> String {
        try stl(tris).write(to: url)
        return url.path(percentEncoded: false)
    }

    static func write(_ tris: [[Float]], name: String) throws -> String {
        try write(tris, to: URL(fileURLWithPath: NSTemporaryDirectory() + name))
    }

    /// A 40 mm cube: it sits flat with nothing to support.
    static func cube() throws -> Part {
        try #require(try loadParts(path: try write(box(-20, 20, -20, 20, 0, 40), name: "tools-cube.stl")).first)
    }

    /// A T: a stem under a slab, so the slab's underside is an overhang.
    static func tShape() throws -> Part {
        try #require(try loadParts(path: try write(box(-5, 5, -5, 5, 0, 20) + box(-20, 20, -20, 20, 20, 24), name: "tools-t.stl")).first)
    }
}

/// A pose held in a box, as the models see the viewer's.
@MainActor
final class FakePose {
    var rotation = PoseAccess.identity
    private(set) var applied: [[Double]] = []
    var access: PoseAccess {
        PoseAccess(rotation: { self.rotation }, apply: { r in self.rotation = r; self.applied.append(r) })
    }
}

/// Three.js `Euler(x, y, z, 'XYZ')` in degrees as nine column-major numbers.
func eulerMatrix(_ x: Double, _ y: Double, _ z: Double) -> [Double] {
    let r = { (d: Double) in d * Double.pi / 180 }
    let (cx, sx, cy, sy, cz, sz) = (cos(r(x)), sin(r(x)), cos(r(y)), sin(r(y)), cos(r(z)), sin(r(z)))
    return [
        cy * cz, cx * sz + sx * sy * cz, sx * sz - cx * sy * cz,
        -cy * sz, cx * cz - sx * sy * sz, sx * cz + cx * sy * sz,
        sy, -sx * cy, cx * cy,
    ]
}

/// Poll `condition` on the main actor until it holds (the engine's worker answers on its own thread).
@MainActor
func eventually(_ seconds: Double = 30, _ condition: () -> Bool) async -> Bool {
    let deadline = ContinuousClock.now + .seconds(seconds)
    while !condition() {
        if ContinuousClock.now > deadline { return false }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return true
}

/// Poll `condition` on the main actor every 10 ms, `tries` times, then record a timeout naming `what`.
@MainActor
func eventually(_ what: String, tries: Int = 1000, _ condition: () -> Bool) async throws {
    for _ in 0..<tries {
        if condition() { return }
        try await Task.sleep(for: .milliseconds(10))
    }
    Issue.record("timed out: \(what)")
}
