import Foundation
import Testing

@testable import Support_Fins

/// The object picker's "Load N as separate models" (the app's own; the web only merges): several objects of one file
/// as models of their own on one plate, each where it stands in the file, the default when they stand apart. Plate ties
/// need them so (the user's file of ten coins, merged, had nothing to tie).
@MainActor
@Suite("Separate models from one file")
struct SeparateModelsTests {
    /// An OBJ of objects: each a list of triangles (9 numbers each), named.
    static func obj(_ objects: [(name: String, tris: [[Float]])], file: String) throws -> URL {
        var text = "", base = 1
        for o in objects {
            text += "o \(o.name)\n"
            for t in o.tris {
                for k in 0..<3 { text += "v \(t[3 * k]) \(t[3 * k + 1]) \(t[3 * k + 2])\n" }
                text += "f \(base) \(base + 1) \(base + 2)\n"
                base += 3
            }
        }
        let url = try Scratch.directory().appending(path: "\(file)-\(UUID().uuidString).obj")
        try text.write(to: url, atomically: true, encoding: .utf8)
        return url
    }

    /// Three 10 mm cubes along x at 0, 15 and 40 mm (gaps of 5 and 15), the middle one 20 mm up y.
    static func cubesApart() throws -> URL {
        try obj([("a", TestParts.box(0, 10, 0, 10, 0, 10)), ("b", TestParts.box(15, 25, 20, 30, 0, 10)),
                 ("c", TestParts.box(40, 50, 0, 10, 0, 10))], file: "apart")
    }

    // MARK: the picker

    private func picker(bounds: [[Double]]?) -> ObjectPickerModel {
        let p = ObjectPickerModel(objects: (0..<3).map { (name: "o\($0)", triangles: 12, size: [10.0, 10, 10]) }, bounds: bounds)
        p.setAll(true)
        return p
    }

    @Test func objectsThatStandApartLoadSeparatelyByDefault() {
        let p = picker(bounds: [[0, 0, 0, 10, 10, 10], [10, 0, 0, 20, 10, 10], [30, 0, 0, 40, 10, 10]])
        #expect(p.standApart, "touching boxes stand apart")
        #expect(p.loadTitle == "Load 3 as separate models" && p.otherTitle == "Merge 3 into one part")
        #expect(p.hint == "3 selected — each its own model, where it stands in the file")
        var merged: [Int]?, separate: [Int]?
        p.onLoad = { merged = $0 }
        p.onLoadSeparately = { separate = $0 }
        p.load()
        #expect(separate == [0, 1, 2] && merged == nil)
        p.loadOther()
        #expect(merged == [0, 1, 2])
    }

    @Test func overlappingObjectsMergeByDefault() {
        let p = picker(bounds: [[0, 0, 0, 10, 10, 10], [5, 5, 5, 15, 15, 15], [30, 0, 0, 40, 10, 10]])
        #expect(!p.standApart)
        #expect(p.loadTitle == "Merge 3 into one part" && p.otherTitle == "Load 3 as separate models")
        #expect(p.hint == "3 selected — merged into one part")
        var separate: [Int]?
        p.onLoadSeparately = { separate = $0 }
        p.loadOther()
        #expect(separate == [0, 1, 2])
        // the overlapping one unchecked: the other two stand apart
        p.set(1, false)
        #expect(p.standApart && p.loadTitle == "Load 2 as separate models")
    }

    @Test func unknownBoxesOrOneObjectKeepTheWebsLoad() {
        let p = picker(bounds: nil)
        #expect(!p.standApart && p.loadTitle == "Merge 3 into one part")
        p.setAll(false)
        p.set(2, true)
        #expect(p.loadTitle == "Load" && p.otherTitle == nil && p.hint.isEmpty)
        var one: [Int]?
        p.onLoad = { one = $0 }
        p.load(separately: true)
        #expect(one == [2], "one object is one load, whichever way")
    }

    // MARK: the window

    @Test func theFilesObjectsBecomeModelsWhereTheyStand() async throws {
        let model = ViewerModel()
        try #require(model.controller != nil, "no Metal device")
        let url = try Self.cubesApart()
        defer { Scratch.remove(url) }
        await model.load(url: url)
        let picker = try #require(model.picker)
        #expect(picker.entries.allSatisfy { $0.bounds != nil }, "the picker knows each object's box")
        picker.setAll(true)
        #expect(picker.separateByDefault)
        picker.load()
        let board = try #require(model.features[PlatesFeature.self]?.board)
        #expect(await eventually { model.picker == nil && !model.isLoading && board.modelCount == 3 })
        #expect(board.plates.count == 1 && board.plate.models.count == 3)
        // file order, each where it stands: the group's box (0..50 x 0..30) centred on the bed
        let xy = board.plate.models.map(\.xy)
        #expect(xy == [SIMD2(-20, -10), SIMD2(-5, 10), SIMD2(20, -10)], "\(xy)")
        // each model is its own part, named after its object
        #expect(model.session.parts.count == 1 && model.session.parts[0].info.name == "c")
        #expect(board.overlapping == false, "nothing clashes")
    }

    @Test func coinsLoadedSeparatelyAreTiedPairByPair() async throws {
        // three upright coins (r 20, 3 mm thick, faces along x) 4 mm apart, one object each
        func coin(at x: Float) -> [[Float]] {
            let n = 96, r: Float = 20, h: Float = 1.5
            func p(_ px: Float, _ i: Int) -> [Float] {
                let a = Float(i % n) / Float(n) * 2 * .pi
                return [x + px, r * cos(a), r + r * sin(a)]
            }
            let c = { (px: Float) -> [Float] in [x + px, 0, r] }
            var tris: [[Float]] = []
            for i in 0..<n {
                tris.append(c(h) + p(h, i) + p(h, i + 1))
                tris.append(c(-h) + p(-h, i + 1) + p(-h, i))
                tris.append(p(-h, i) + p(-h, i + 1) + p(h, i + 1))
                tris.append(p(-h, i) + p(h, i + 1) + p(h, i))
            }
            return tris
        }
        let url = try Self.obj([("c1", coin(at: -7)), ("c2", coin(at: 0)), ("c3", coin(at: 7))], file: "coins")
        defer { Scratch.remove(url) }
        let model = ViewerModel()
        try #require(model.controller != nil, "no Metal device")
        await model.load(url: url)
        let picker = try #require(model.picker)
        picker.setAll(true)
        picker.load()
        let board = try #require(model.features[PlatesFeature.self]?.board)
        #expect(await eventually { model.picker == nil && !model.isLoading && board.modelCount == 3 })
        let ties = try #require(model.features[PlateTiesFeature.self]?.model)
        ties.setOn(true)
        #expect(await ties.settled())
        #expect(ties.error == nil, "\(ties.error ?? "")")
        #expect(ties.links.count == 2 && ties.placed == ties.requested && ties.placed > 0, "\(ties.links.count) pairs, \(ties.placed)/\(ties.requested)")
        ties.setOn(false)
    }
}
