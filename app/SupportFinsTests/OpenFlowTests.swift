import Foundation
import Testing

@testable import Support_Fins

/// The open flow (`ui/io.js` `pickObjects`, `#drop`): several objects ask which to load; Cancel loads nothing.
@MainActor
@Suite("Open flow")
struct OpenFlowTests {
    /// An OBJ of three boxes (a 10 mm cube, a 20 mm cube and a 5 mm cube): `load_parts` gives one `Part` each.
    static func threeObjects() throws -> URL {
        var obj = ""
        var base = 1
        for (name, e) in [("small", 10), ("big", 20), ("tiny", 5)] {
            obj += "o \(name)\n"
            for z in [0, e] { for y in [0, e] { for x in [0, e] { obj += "v \(x) \(y) \(z)\n" } } }
            for f in [[1, 3, 2], [2, 3, 4], [5, 6, 7], [6, 8, 7], [1, 2, 6], [1, 6, 5], [2, 4, 8], [2, 8, 6], [4, 3, 7], [4, 7, 8], [3, 1, 5], [3, 5, 7]] {
                obj += "f " + f.map { String($0 + base - 1) }.joined(separator: " ") + "\n"
            }
            base += 8
        }
        let url = URL(fileURLWithPath: NSTemporaryDirectory() + "open-flow-three.obj")
        try obj.write(to: url, atomically: true, encoding: .utf8)
        return url
    }

    @Test func aFileWithSeveralObjectsShowsThePickerAndLoadsNothingYet() async throws {
        let model = ViewerModel()
        await model.load(url: try Self.threeObjects())
        let picker = try #require(model.picker)
        #expect(!model.hasPart)
        // equal triangle counts keep the file's order; the first is pre-selected (`io.js:151-161`)
        #expect(picker.entries.map(\.name) == ["small", "big", "tiny"])
        #expect(picker.entries.map(\.triangles) == [12, 12, 12] && picker.entries[1].size == [20, 20, 20])
        #expect(picker.selected == [picker.entries[0].id] && picker.canLoad && picker.loadTitle == "Load")
    }

    @Test func aSingleChoiceLoadsThatObject() async throws {
        let model = ViewerModel()
        await model.load(url: try Self.threeObjects())
        let picker = try #require(model.picker)
        let small = try #require(picker.entries.first { $0.name == "small" })
        picker.set(picker.entries[0].id, false)
        #expect(!picker.canLoad)                       // Load stays off until one is checked
        picker.set(small.id, true)
        picker.load()
        #expect(model.picker == nil && model.hasPart && model.status == nil)
        #expect(model.session.parts.count == 1 && model.session.parts[0].info.name == "small")
        #expect(model.session.parts[0].info.importNote == "OBJ: imported “small” of 3 objects.")
        #expect(model.controller?.size == SIMD3<Float>(10, 10, 10))
    }

    @Test func cancelLeavesTheWindowEmpty() async throws {
        let model = ViewerModel()
        await model.load(url: try Self.threeObjects())
        model.picker?.cancel()
        #expect(model.picker == nil && !model.hasPart && model.session.parts.isEmpty && model.hasOpened)
    }

    @Test func cancelKeepsThePartAlreadyOnShow() async throws {
        let model = ViewerModel()
        await model.load(url: try TestParts.write(TestParts.box(0, 30, 0, 30, 0, 30), name: "open-flow-one.stl").asURL)
        await model.load(url: try Self.threeObjects())
        model.picker?.cancel()
        #expect(model.hasPart && model.session.parts[0].info.name == "open-flow-one")
    }

    @Test func severalChoicesLoadOneMergedPart() async throws {
        let model = ViewerModel()
        await model.load(url: try Self.threeObjects())
        let picker = try #require(model.picker)
        picker.set(picker.entries[1].id, true)
        #expect(picker.loadTitle == "Merge 2 & load" && picker.hint == "2 selected — merged into one part")
        picker.load()
        #expect(await eventually { model.hasPart })
        #expect(model.picker == nil && model.status == nil)
        // small (10 mm) and big (20 mm) cubes at the origin: 12 + 12 triangles, one part, the union's box
        #expect(model.session.parts.count == 1)
        #expect(model.session.parts[0].info.notes == ["merged 2 of 3 objects into one part"])
        #expect(model.session.parts[0].info.importNote == "OBJ: merged 2 of 3 objects into one part.")
        #expect(model.session.parts[0].info.triangles == 24 && model.controller?.faceCount == 24)
        #expect(model.controller?.size == SIMD3<Float>(20, 20, 20))
    }

    @Test func aSingleObjectFileSkipsThePicker() async throws {
        let model = ViewerModel()
        await model.load(url: try TestParts.write(TestParts.box(0, 30, 0, 30, 0, 30), name: "open-flow-solo.stl").asURL)
        #expect(model.picker == nil && model.hasPart)
        #expect(model.session.parts[0].info.importNote.isEmpty)    // an STL has no import note
    }

    @Test func theStatusCardListsItsLinesInThePagesOrder() {
        let lines = StatusCard.lines(importNote: "OBJ: imported “b” of 2 objects.", flat: "flat", flatGood: true,
                                     fin: "fin", overWarn: "over", island: "island")
        #expect(lines.map(\.text) == ["OBJ: imported “b” of 2 objects.", "flat", "fin", "over", "island"])
        #expect(lines.map(\.bar) == [nil, nil, nil, StatusCard.amber, StatusCard.red], "the warning cards' bars")
        #expect(StatusCard.lines(importNote: "", flat: "", flatGood: false, fin: "", overWarn: "", island: nil).isEmpty)
    }

    @Test func largestByTrianglesFirstAndPreselected() {
        let p = ObjectPickerModel(objects: [("a", 5, [1, 2, 3]), ("b", 5000, [40, 40.4, 40.6]), ("c", 20, [1, 1, 1])])
        #expect(p.entries.map(\.name) == ["b", "c", "a"] && p.selected == [1])
        #expect(p.entries[0].meta == "5,000 tris · 40×40×41 mm" && p.hint.isEmpty)
    }

    @Test func ourOwnSupportsObjectGoesLast() {
        let p = ObjectPickerModel(objects: [("bracket supports", 900, [1, 1, 1]), ("bracket", 100, [1, 1, 1])])
        #expect(p.entries.map(\.name) == ["bracket", "bracket supports"] && p.selected == [1])
    }

    @Test func dropsTakeSTLOBJ3MFAndSTEP() {
        for n in ["a.stl", "a.STL", "b.obj", "c.3mf", "d.step", "e.STP"] { #expect(OpenRules.accepts(URL(fileURLWithPath: "/x/" + n)), "\(n)") }
        for n in ["a.png", "a.iges", "a.stepx"] { #expect(!OpenRules.accepts(URL(fileURLWithPath: "/x/" + n)), "\(n)") }
    }
}

private extension String { var asURL: URL { URL(fileURLWithPath: self) } }
