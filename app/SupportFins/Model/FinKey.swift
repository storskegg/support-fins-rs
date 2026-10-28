import Foundation

/// A fin's spatial key: how a removed Auto fin is found again in a fresh build (`ui/remove.js`).
/// A port of `web/ui/finkeys.js`, whose header explains the tolerances: the key is the fin's kind, the
/// point halfway along its line, its height and its bearing, rounded to 0.1 mm, and a removal matches the
/// NEAREST fin of the same kind and bearing within a tolerance (erring towards a removed fin coming back,
/// never towards a kept fin vanishing). The text format is the web's (`kind:x,y,z:height:bearing`), so a
/// saved session's keys still parse.
struct FinKey: Hashable, Codable, Sendable, CustomStringConvertible {
    /// `kind:x,y,z:height:bearing`, with a `#n` suffix on the n-th repeat of one key in a build.
    let raw: String

    init(raw: String) { self.raw = raw }
    var description: String { raw }

    // MARK: Tolerances (finkeys.js:23-27)
    /// mm a fin may move across its own line and still be the same fin
    static let acrossTolerance = 0.4
    /// mm its middle may slide along the line (its ends grew or shrank)
    static let alongTolerance = 2.5
    /// mm its height may change (a gap step: 0.1)
    static let heightTolerance = 0.35
    /// mm up or down: a gap step moves a wall top ~0.1-0.2
    static let zTolerance = 1.0
    /// rad
    static let bearingTolerance = 0.15

    /// `{kind, x, y, z, h, bearing}` from a key (`parseKey`, finkeys.js:88-91). The `#n` / `~n` suffixes that
    /// keep keys unique are ignored. `nil` for a text that is not a key.
    struct Parsed: Equatable, Sendable {
        var kind: String
        var x: Double, y: Double, z: Double, h: Double, bearing: Double
    }

    var parsed: Parsed? {
        guard let m = raw.wholeMatch(of: /([^:]+):(-?[\d.]+),(-?[\d.]+),(-?[\d.]+):(-?[\d.]+):(-?[\d.]+)(?:[#~]\d+)*/) else { return nil }
        // JS `+m[i]`: a malformed number (`1.2.3`) is NaN
        let n = { (s: Substring) in Double(s) ?? .nan }
        return Parsed(kind: String(m.1), x: n(m.2), y: n(m.3), z: n(m.4), h: n(m.5), bearing: n(m.6))
    }

    /// A fin record's key (`finKey`, finkeys.js:55-62): kind, mid-length point, height, bearing. `nil` for a record
    /// with no line. `legacy` uses the middle VERTEX, what keys saved before 045 were taken at.
    init?(record r: FinRecord, legacy: Bool = false) {
        guard let first = r.line.first, let last = r.line.last else { return nil }
        let m = legacy ? r.line[r.line.count / 2] : Self.midAlong(r.line)
        let bearing = atan2(last.y - first.y, last.x - first.x)
        raw = "\(r.kind):\(Self.js(Self.r1(m.x))),\(Self.js(Self.r1(m.y))),\(Self.js(Self.r1(m.z)))"
            + ":\(Self.js(Self.r1(r.height))):\(Self.js(Self.r1(bearing)))"
    }

    /// The point halfway along a polyline's length (not its middle vertex, which jumps when the line gains a
    /// point) (`midAlong`, finkeys.js:32-47).
    static func midAlong(_ line: [Point3]) -> Point3 {
        if line.count == 1 { return line[0] }
        var total = 0.0
        for i in 1..<line.count { total += hypot(line[i].x - line[i - 1].x, line[i].y - line[i - 1].y) }
        var left = total / 2
        for i in 1..<line.count {
            let a = line[i - 1], b = line[i]
            let seg = hypot(b.x - a.x, b.y - a.y)
            if seg >= left && seg > 0 {
                let t = left / seg
                return Point3(x: a.x + t * (b.x - a.x), y: a.y + t * (b.y - a.y), z: a.z + t * (b.z - a.z))
            }
            left -= seg
        }
        return line[line.count - 1]
    }

    /// Key each record, UNIQUELY: a symmetric part can give two fins one key, and a shared key would make one
    /// removal click drop both. A repeat gets `#n` (`assignKeys`, finkeys.js:66-77). `nil` where a record has
    /// no line. Also returns each record's legacy key, matched exactly.
    static func assign(_ records: [FinRecord]) -> [Assigned] {
        var seen: [String: Int] = [:]
        return records.map { r in
            guard let base = FinKey(record: r) else { return Assigned(sig: nil, legacySig: nil) }
            let n = seen[base.raw, default: 0]
            seen[base.raw] = n + 1
            return Assigned(sig: n == 0 ? base : FinKey(raw: "\(base.raw)#\(n)"), legacySig: FinKey(record: r, legacy: true))
        }
    }

    struct Assigned: Equatable, Sendable {
        var sig: FinKey?
        var legacySig: FinKey?
    }

    /// Match removed keys to a build's records: a key that names a record exactly takes it (a `#n` twin is told
    /// apart only by its suffix); otherwise the nearest record of its kind and bearing within the tolerances,
    /// one record per key, closest pairs first. An older session's key (middle vertex), or one that doesn't
    /// parse, matches exactly only. A key with no fin near it (another pose, or a fin the new build doesn't
    /// make) matches nothing and is kept for later. Returns record id -> the removed key it matched
    /// (`matchRemoved`, finkeys.js:104-128).
    ///
    /// JS walks its `Set` in insertion order; `removed` here is unordered, so keys are walked in text order,
    /// which only matters between two pairs at exactly equal distance.
    static func matchRemoved(records: [FinRecord], keys: [Assigned], removed: Set<FinKey>) -> [Int: FinKey] {
        var pairs: [(score: Double, key: FinKey, id: Int)] = []
        let parsed = zip(records, keys).map { r, k in (r, k.sig?.parsed) }.compactMap { r, q in q.map { (r, $0) } }
        for k in removed.sorted(by: { $0.raw < $1.raw }) {
            for (r, a) in zip(records, keys) where k == a.sig || k == a.legacySig { pairs.append((-1, k, r.id)) }
            guard let p = k.parsed else { continue }
            for (r, q) in parsed {
                if p.kind != q.kind || bearingGap(p.bearing, q.bearing) > bearingTolerance { continue }
                let dx = q.x - p.x, dy = q.y - p.y, c = cos(p.bearing), s = sin(p.bearing)
                let along = abs(dx * c + dy * s), across = abs(dy * c - dx * s), dz = abs(p.z - q.z)
                if across > acrossTolerance || along > alongTolerance || dz > zTolerance || abs(p.h - q.h) > heightTolerance { continue }
                // across weighs more: it's the neighbour axis
                pairs.append((hypot(hypot(along, 4 * across), dz), k, r.id))
            }
        }
        // `Array.sort` is stable; Swift's `sort` is not guaranteed to be, so the index breaks ties
        let ordered = pairs.enumerated().sorted { ($0.element.score, $0.offset) < ($1.element.score, $1.offset) }.map(\.element)
        var out: [Int: FinKey] = [:], used = Set<FinKey>()
        for p in ordered {
            if used.contains(p.key) || out[p.id] != nil { continue }
            used.insert(p.key)
            out[p.id] = p.key
        }
        return out
    }

    /// Bearing difference of two undirected lines (a rebuild may run a wall the other way), in [0, pi/2].
    static func bearingGap(_ a: Double, _ b: Double) -> Double {
        let d = fmod(abs(a - b), .pi)
        return min(d, .pi - d)
    }

    // MARK: JavaScript number formatting

    /// `Math.round(v * 10) / 10`: 0.1 mm (JS rounds halves up, toward +infinity).
    static func r1(_ v: Double) -> Double {
        let x = v * 10
        let f = x.rounded(.down)
        return (x - f >= 0.5 ? f + 1 : f) / 10
    }

    /// A number as a JS template literal prints it (`${v}`): integers without ".0", `-0` as `0`.
    /// The values here are multiples of 0.1 well inside +-1e21, where Swift's shortest-round-trip digits are JS's.
    static func js(_ v: Double) -> String {
        if v == 0 { return "0" }
        if v == v.rounded() && abs(v) < 1e21 { return String(Int64(v)) }
        return "\(v)"
    }
}
