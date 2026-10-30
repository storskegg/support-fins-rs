import Foundation
import Metal
import Testing

@testable import SupportFinsCore

private let settings = """
{"tines":true,"tineDensity":1,"layerHeight":0.2,"plateOnly":false,"tallRibs":true,\
"sway":{"gripFrom":0,"tineSpacing":6,"reach":0.15,"uncappedDepth":false,"crossRise":0.5}}
"""

/// The Draw interface (docs/FFI.md "M6 additions: Draw") as the app calls it: a kit for a pose, a wall stood from two
/// points on the surface, the pointer ray's spot, a refused wall's reason.
@Test func aWallStandsBetweenTwoPointsOnTheSurface() throws {
    let part = try #require(try loadParts(path: try modelPath("cube")).first)
    let s = sin(35 * Double.pi / 180), c = cos(35 * Double.pi / 180)
    let rot = [1, 0, 0, 0, c, s, 0, -s, c]
    let kit = try DrawKit(part: part, rot: rot, thresholdDeg: 45, settingsJson: settings, tunablesJson: "{}")
    #expect(kit.offset().count == 3)

    // two points on the cube's tilted underside: rays from below, the id pass's face found by trying each
    func under(_ x: Double) throws -> [Double] {
        var best: (d: Double, p: [Double])?
        for f in 0..<12 {
            if let h = try kit.surfaceHit(face: UInt64(f), origin: [x, 0, -50], dir: [0, 0, 1]), best == nil || h.distance < best!.d {
                best = (h.distance, h.point)
            }
        }
        return try #require(best).p
    }
    let a = try kit.worldToLocal(p: try under(-6)), b = try kit.worldToLocal(p: try under(6))
    let walls = "[{\"kind\":\"wall\",\"a\":\(a),\"b\":\(b)}]"
    let sweep = try kit.sweep(wallsJson: walls, autoBuild: nil, removed: [])
    let json = try #require(try JSONSerialization.jsonObject(with: Data(sweep.wallsJson().utf8)) as? [[String: Any]])
    #expect(json.count == 1 && json[0]["ok"] as? Bool == true)
    #expect(sweep.triangles().count() > 0)

    // a wall of half a millimetre is refused, with the web's words
    let tiny = try kit.checkWall(a: try under(0), b: (try under(0)).enumerated().map { $0 == 0 ? $1 + 0.5 : $1 }, m: nil)
    #expect(!tiny.ok() && tiny.reason().hasPrefix("wall too short"))
    // a curve through three points in line is the straight wall; the band of it is its two ends
    #expect(try arcBand(a: [0, 0, 0], b: [10, 0, 0], m: [5, 0, 0]).count == 6)
}
