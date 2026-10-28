import Testing

@testable import Support_Fins

private func record(_ id: Int, kind: String = "prop", height: Double = 6, line: [(Double, Double, Double)]) -> FinRecord {
    FinRecord(id: id, kind: kind, height: height, length: 10, tines: 3, rows: 1, stilt: 0, lean: 0, bearing: 0, span: 10,
              short: false, fill: false, joined: false, braces: nil, ribs: nil, triRanges: [VertexRange(from: id * 36, to: id * 36 + 36)],
              line: line.map { Point3(x: $0.0, y: $0.1, z: $0.2) })
}

/// A straight wall along +x from `x0` to `x1` at height z = 5, y = `y`.
private func wall(_ id: Int, _ x0: Double, _ x1: Double, y: Double = 0, height: Double = 6, kind: String = "prop") -> FinRecord {
    record(id, kind: kind, height: height, line: [(x0, y, 5), ((x0 + x1) / 2, y, 5), (x1, y, 5)])
}

@Suite("FinKey")
struct FinKeyTests {
    @Test func theKeyIsTheWebsText() {
        // kind:x,y,z of the middle of the line:height:bearing, each to 0.1 mm (finkeys.js:55-62)
        let r = wall(0, 0, 10, y: 3.04)
        #expect(FinKey(record: r)?.raw == "prop:5,3,5:6:0")
        // JS formats whole numbers without ".0" and -0 as 0; Math.round rounds halves up
        #expect(FinKey(record: wall(0, -10.04, 0, y: 0))?.raw == "prop:-5,0,5:6:0")
        let neg = record(0, height: 6.25, line: [(0, -0.05, 5), (1, -0.05, 5)])
        #expect(FinKey(record: neg)?.raw == "prop:0.5,0,5:6.3:0") // r1(-0.05) is -0, printed 0; r1(6.25) is 6.3
        #expect(FinKey(record: record(0, line: []))  == nil)
    }

    @Test func midAlongIsHalfwayByLengthNotTheMiddleVertex() {
        // a line whose middle vertex is far from its length-midpoint
        let line = [Point3(x: 0, y: 0, z: 0), Point3(x: 1, y: 0, z: 0), Point3(x: 10, y: 0, z: 0)]
        #expect(FinKey.midAlong(line) == Point3(x: 5, y: 0, z: 0))
        let r = record(0, line: [(0, 0, 0), (1, 0, 0), (10, 0, 0)])
        #expect(FinKey(record: r)?.raw.hasPrefix("prop:5,0,0:") == true)
        #expect(FinKey(record: r, legacy: true)?.raw.hasPrefix("prop:1,0,0:") == true) // the middle VERTEX
        #expect(FinKey.midAlong([Point3(x: 2, y: 3, z: 4)]) == Point3(x: 2, y: 3, z: 4))
    }

    @Test func aRepeatedKeyGetsANumberedSuffix() {
        let twins = [wall(0, 0, 10), wall(1, 0, 10), wall(2, 0, 10), wall(3, 20, 30)]
        let keys = FinKey.assign(twins).compactMap(\.sig?.raw)
        #expect(keys == ["prop:5,0,5:6:0", "prop:5,0,5:6:0#1", "prop:5,0,5:6:0#2", "prop:25,0,5:6:0"])
        #expect(FinKey.assign([record(9, line: [])]) == [FinKey.Assigned(sig: nil, legacySig: nil)])
    }

    @Test func parsingIgnoresTheSuffixAndRejectsOtherText() {
        let p = FinKey(raw: "wedge:-1.5,2,3.2:4.1:-0.6#2").parsed
        #expect(p == FinKey.Parsed(kind: "wedge", x: -1.5, y: 2, z: 3.2, h: 4.1, bearing: -0.6))
        #expect(FinKey(raw: "prop:1,2,3:4:5~7").parsed?.bearing == 5)
        #expect(FinKey(raw: "nonsense").parsed == nil)
        #expect(FinKey(raw: "prop:1,2:4:5").parsed == nil)
        #expect(FinKey(raw: "prop:1.2.3,2,3:4:5").parsed?.x.isNaN == true) // JS `+"1.2.3"` is NaN
    }

    @Test func aRemovedFinIsFoundAgainWithinTheTolerances() {
        let before = [wall(0, 0, 10), wall(1, 0, 10, y: 8)]
        let removed = Set(FinKey.assign(before).compactMap(\.sig).prefix(1)) // the wall at y = 0
        // a rebuild nudges it: 1 mm along, 0.3 mm across, 0.1 mm taller
        let after = [wall(10, 1, 11, y: 0.3, height: 6.1), wall(11, 1, 11, y: 8.3, height: 6.1)]
        let m = FinKey.matchRemoved(records: after, keys: FinKey.assign(after), removed: removed)
        #expect(m.count == 1 && m[10] == removed.first)
    }

    @Test func theToleranceErrsTowardsARemovedFinComingBack() {
        let removed: Set<FinKey> = [FinKey(raw: "prop:5,0,5:6:0")]
        func matches(_ r: FinRecord) -> Bool {
            !FinKey.matchRemoved(records: [r], keys: FinKey.assign([r]), removed: removed).isEmpty
        }
        #expect(matches(wall(0, 0, 10)))
        #expect(!matches(wall(0, 0, 10, y: 0.5)), "more than 0.4 mm across is a neighbour")
        #expect(!matches(wall(0, 3, 13)), "more than 2.5 mm along")
        #expect(!matches(wall(0, 0, 10, height: 6.4)), "more than 0.35 mm taller")
        #expect(!matches(wall(0, 0, 10, kind: "wedge")), "another kind")
        // bearing beyond 0.15 rad: the same wall turned about 10 degrees
        let turned = record(0, line: [(0, 0, 5), (5, 0.9, 5), (10, 1.8, 5)])
        #expect(!matches(turned))
        // a wall run the other way is the same undirected line
        let reversed = record(0, line: [(10, 0, 5), (5, 0, 5), (0, 0, 5)])
        #expect(matches(reversed))
    }

    @Test func oneRecordPerKeyAndTheClosestPairWins() {
        // two removed keys, two candidate walls: each takes the nearer one, never both the same
        let removed: Set<FinKey> = [FinKey(raw: "prop:5,0,5:6:0"), FinKey(raw: "prop:5,0.5,5:6:0")]
        let recs = [wall(0, 0, 10, y: 0.05), wall(1, 0, 10, y: 0.45)]
        let m = FinKey.matchRemoved(records: recs, keys: FinKey.assign(recs), removed: removed)
        #expect(m.count == 2 && m[0]?.raw == "prop:5,0,5:6:0" && m[1]?.raw == "prop:5,0.5,5:6:0")
        // an exact match always wins (score -1), twin suffixes told apart by the suffix
        let twins = [wall(0, 0, 10), wall(1, 0, 10)]
        let tk = FinKey.assign(twins)
        let second = tk[1].sig!
        let m2 = FinKey.matchRemoved(records: twins, keys: tk, removed: [second])
        #expect(m2 == [1: second])
    }

    @Test func aLegacyKeyMatchesExactlyOnly() {
        let r = record(0, line: [(0, 0, 5), (1, 0, 5), (10, 0, 5)])
        let keys = FinKey.assign([r])
        let legacy = keys[0].legacySig!
        #expect(FinKey.matchRemoved(records: [r], keys: keys, removed: [legacy]) == [0: legacy])
        // the same text nudged by 0.1 does not parse-match through the legacy path, but the tolerance finds it
        #expect(FinKey.r1(0.25) == 0.3 && FinKey.r1(-0.25) == -0.2) // halves round up, as Math.round
    }
}
