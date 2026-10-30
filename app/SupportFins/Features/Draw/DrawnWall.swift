import Foundation
import simd

/// A hand-placed support in the part's own frame, so it follows the part through later turns (`drawnWalls`,
/// `web/ui/walls.js:27`): a breakaway wall from `a` to `b` (a curve also runs through `m`), or a sway brace
/// stood on `face` at the spot `a`.
struct DrawnWall: Equatable, Identifiable, Sendable {
    enum Kind: String, Sendable { case wall, sway }

    /// Names it across a rebuild, an undo and a selection (the web holds the wall object).
    let id: UUID
    var kind: Kind
    var a: SIMD3<Double>
    var b: SIMD3<Double>
    var m: SIMD3<Double>?
    var face: Int
    /// A brace that stops at the height it was clicked at (`capped`); the rest run up the whole face.
    var capped: Bool

    static func wall(from a: SIMD3<Double>, to b: SIMD3<Double>, through m: SIMD3<Double>? = nil, id: UUID = UUID()) -> DrawnWall {
        DrawnWall(id: id, kind: .wall, a: a, b: b, m: m, face: 0, capped: false)
    }

    static func brace(at a: SIMD3<Double>, face: Int, capped: Bool, id: UUID = UUID()) -> DrawnWall {
        DrawnWall(id: id, kind: .sway, a: a, b: a, m: nil, face: face, capped: capped)
    }

    /// The `walls_json` entry (docs/FFI.md, "M6 additions: Draw").
    var json: String {
        func v(_ p: SIMD3<Double>) -> String { "[\(JS.number(p.x)),\(JS.number(p.y)),\(JS.number(p.z))]" }
        switch kind {
        case .wall:
            return "{\"kind\":\"wall\",\"a\":\(v(a)),\"b\":\(v(b))" + (m.map { ",\"m\":\(v($0))" } ?? "") + "}"
        case .sway:
            return "{\"kind\":\"sway\",\"a\":\(v(a)),\"face\":\(face),\"capped\":\(capped)}"
        }
    }
}

extension Collection where Element == DrawnWall {
    /// `walls_json`.
    var json: String { "[" + map(\.json).joined(separator: ",") + "]" }
}

/// What `DrawnSweep.walls_json()` says of one wall (`w.ok` and `w.info`, `web/ui/walls.js` `sweepDrawn`).
struct WallOutcome: Decodable, Equatable, Sendable {
    var ok: Bool
    /// Triangle indices into the sweep's triangles.
    var triStart: Int
    var triEnd: Int
    var kind: String?
    /// Why it could not be built (`info.reason`).
    var reason: String?
    var length = 0.0
    var height = 0.0
    var tines = 0
    /// A wall's face ribs (`info.braces`).
    var ribs = 0
    /// A brace's cross ribs (`info.cross`).
    var cross = 0
    /// A brace's stilt (`info.stilt`).
    var stilt = 0.0

    private enum Key: String, CodingKey { case ok, triStart, triEnd, kind, reason, length, height, tines, ribs, cross, stilt }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        ok = try c.decode(Bool.self, forKey: .ok)
        triStart = try c.decode(Int.self, forKey: .triStart)
        triEnd = try c.decode(Int.self, forKey: .triEnd)
        kind = try c.decodeIfPresent(String.self, forKey: .kind)
        reason = try c.decodeIfPresent(String.self, forKey: .reason)
        length = try c.decodeIfPresent(Double.self, forKey: .length) ?? 0
        height = try c.decodeIfPresent(Double.self, forKey: .height) ?? 0
        tines = try c.decodeIfPresent(Int.self, forKey: .tines) ?? 0
        ribs = try c.decodeIfPresent(Int.self, forKey: .ribs) ?? 0
        cross = try c.decodeIfPresent(Int.self, forKey: .cross) ?? 0
        stilt = try c.decodeIfPresent(Double.self, forKey: .stilt) ?? 0
    }

    static func decode(_ json: String) -> [WallOutcome] {
        (try? JSONDecoder().decode([WallOutcome].self, from: Data(json.utf8))) ?? []
    }
}

extension FinSettings {
    /// `DrawKit`'s `settings_json`: what `sweepDrawn` and `swayOpts()` read off the page's controls
    /// (`web/ui/walls.js:253-260`, `web/ui/finbuild.js:156-170`). The percent fields are divided here, in the
    /// same expression the page uses, so the doubles are the page's.
    func drawSettingsJSON() -> String {
        func n(_ x: Double) -> String { x == x.rounded() && abs(x) < 1e15 ? String(Int64(x)) : "\(x)" }
        let sway = "{\"gripFrom\":\(n(swayFrom)),\"tineSpacing\":\(n(swaySpacing)),\"reach\":\(n(swayDepth / 100)),"
            + "\"uncappedDepth\":\(fullBraceDepth),\"crossRise\":\(n(Swift.max(10, Swift.min(90, swayCrossHeight)) / 100))}"
        return "{\"tines\":\(tines),\"tineDensity\":\(n(tineDensity / 100)),\"layerHeight\":\(n(layerHeight)),"
            + "\"plateOnly\":\(plateOnly),\"tallRibs\":\(tallRibs),\"sway\":\(sway)}"
    }
}
