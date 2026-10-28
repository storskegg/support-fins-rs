import Foundation

/// A point `[x, y, z]` in a record's JSON.
struct Point3: Codable, Equatable, Sendable {
    var x: Double, y: Double, z: Double
    init(x: Double, y: Double, z: Double) { (self.x, self.y, self.z) = (x, y, z) }
    init(from decoder: any Decoder) throws {
        var c = try decoder.unkeyedContainer()
        (x, y, z) = (try c.decode(Double.self), try c.decode(Double.self), try c.decode(Double.self))
    }
    func encode(to encoder: any Encoder) throws {
        var c = encoder.unkeyedContainer()
        try c.encode(x); try c.encode(y); try c.encode(z)
    }
}

/// A `[from, to)` range of vertices in the `fins()` buffer (three per triangle).
struct VertexRange: Codable, Equatable, Hashable, Sendable {
    var from: Int, to: Int
    init(from: Int, to: Int) { (self.from, self.to) = (from, to) }
    init(from decoder: any Decoder) throws {
        var c = try decoder.unkeyedContainer()
        (from, to) = (try c.decode(Int.self), try c.decode(Int.self))
    }
    func encode(to encoder: any Encoder) throws {
        var c = encoder.unkeyedContainer()
        try c.encode(from); try c.encode(to)
    }
    var count: Int { to - from }
}

/// One entry of `records_json().fins` (docs/FFI.md "M4b as implemented"; `web/fins.js` `buildFins`'s `fins[i]`).
struct FinRecord: Codable, Equatable, Sendable {
    var id: Int
    /// `prop`, `wedge` or `sway`
    var kind: String
    var height: Double
    var length: Double
    var tines: Int
    var rows: Int
    var stilt: Double
    var lean: Double
    var bearing: Double
    var span: Double
    var short: Bool
    var fill: Bool
    var joined: Bool
    /// Sway braces only
    var braces: Int?
    var ribs: Double?
    /// `[from, to)` vertex ranges into the fin buffer: what per-fin removal cuts out
    var triRanges: [VertexRange]
    var line: [Point3]
}

/// One entry of `records_json().props`: a wall with the region it serves. A fill-pass wall has no `region`.
struct PropRecord: Codable, Equatable, Sendable {
    var id: Int
    var region: Int?
    var tines: Int
    var span: Double
    var height: Double
    var fill: Bool
    var squat: Bool
    var rimWall: Bool
}

/// `records_json()`: `{ fins, props }`.
struct BuildRecords: Codable, Equatable, Sendable {
    var fins: [FinRecord]
    var props: [PropRecord]
    /// `[from, to)` vertex ranges into the `fins()` buffer of the interface crest, the second material
    /// (docs/FFI.md); empty unless the interface material is on.
    var crestRanges: [Range<Int>] = []

    private enum CodingKeys: String, CodingKey { case fins, props, crestRanges }
    init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        fins = try c.decode([FinRecord].self, forKey: .fins)
        props = try c.decode([PropRecord].self, forKey: .props)
        crestRanges = try c.decodeIfPresent([Range<Int>].self, forKey: .crestRanges) ?? []
    }

    init(json: String) throws { self = try JSONDecoder().decode(Self.self, from: Data(json.utf8)) }
    init(fins: [FinRecord], props: [PropRecord], crestRanges: [Range<Int>] = []) {
        (self.fins, self.props, self.crestRanges) = (fins, props, crestRanges)
    }
}

/// The CLI's `--json` stats (`sf_cli::entry::Stats::to_json`).
struct BuildStats: Codable, Equatable, Sendable {
    var overhangRegions: Int
    var finTriangles: Int
    var swayTriangles: Int
    var padTriangles: Int
    var braces: Int
    var props: Int
    var tines: Int
    var pins: Bool
    var swayBraces: Int
    var swayTines: Int
    var swaySkipped: Int
    var swayReason: String?
    var unserved: Double
    var smallHoleRegions: Int
    var floating: Int
    var floatingDrop: Double
}

/// `summary_json()`: what the readout shows (docs/FFI.md "M4b as implemented").
struct BuildSummary: Codable, Equatable, Sendable {
    struct Rejected: Codable, Equatable, Sendable {
        var blocked: Int, tooFewTines: Int, sites: Int, tried: Int
    }
    struct Bare: Codable, Equatable, Sendable {
        var area: Double, total: Double, coverage: Double
    }
    struct Floating: Codable, Equatable, Sendable {
        var faces: Int
        var lowest: Point3
        var drop: Double
    }
    struct Sway: Codable, Equatable, Sendable {
        var count: Int, tines: Int, skipped: Int
        var reason: String?
        var cross: Int
    }

    var stats: BuildStats
    /// The CLI's one-line report
    var report: String
    var offset: Point3
    /// `auto`, `full`, ...
    var mode: String
    var volume: Double
    var tines: Int
    var braceCount: Int?
    var propCount: Int?
    var unserved: Double
    var sagRisk: Bool
    var smallHoleRegions: Int
    var patchCount: Int
    /// Only the counters that counted anything (JS leaves the others absent, #23)
    var patchStats: [String: Int]
    /// Likewise
    var skipped: [String: Int]
    var rejected: Rejected
    var servedRegions: [Int]
    var bare: Bare?
    var floating: [Floating]
    var sway: Sway?
    var tallRibs: Int?
    /// Walls the pose on screen stands inside a bore (the next orientation ranking's `currentBore`)
    var boreWalls: Int?

    init(json: String) throws { self = try JSONDecoder().decode(Self.self, from: Data(json.utf8)) }
}
