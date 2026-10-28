import Foundation
import SupportFinsCore
import Testing
import UniformTypeIdentifiers

@testable import Support_Fins

private func box(_ x0: Float, _ x1: Float, _ y0: Float, _ y1: Float, _ z0: Float, _ z1: Float) -> [[Float]] {
    let v: [[Float]] = [
        [x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0],
        [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1],
    ]
    let faces = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]]
    return faces.map { f in f.flatMap { v[$0] } }
}

private func writeSTL(_ name: String, _ tris: [[Float]]) throws -> String {
    var data = Data(count: 80)
    var count = UInt32(tris.count)
    withUnsafeBytes(of: &count) { data.append(contentsOf: $0) }
    for t in tris {
        for f in [Float](repeating: 0, count: 3) + t { var x = f; withUnsafeBytes(of: &x) { data.append(contentsOf: $0) } }
        data.append(contentsOf: [0, 0])
    }
    let path = NSTemporaryDirectory() + name
    try data.write(to: URL(fileURLWithPath: path))
    return path
}

private let identity: [Double] = [1, 0, 0, 0, 1, 0, 0, 0, 1]
private func tilt(_ degrees: Double) -> [Double] {
    let a = degrees * Double.pi / 180
    return [1, 0, 0, 0, cos(a), sin(a), 0, -sin(a), cos(a)]
}

private struct StubPanel: SavePanel {
    var answer: URL?
    nonisolated(unsafe) static var asked: [(String, UTType)] = []
    func url(suggesting name: String, type: UTType) -> URL? {
        Self.asked.append((name, type))
        return answer
    }
}

@Suite("Export file names")
struct ExportNameTests {
    @Test func nameCarriesNozzleAndLayer() {
        // the vectors of crates/sf-io/tests/exportname.rs, which are web/exportname.js's
        #expect(ExportName.name(stem: "bracket-fins", ext: "3mf", nozzle: 0.6, layerHeight: 0.24) == "bracket-fins-0.6nozzle-0.24layer.3mf")
        #expect(ExportName.name(stem: "p", ext: "stl", nozzle: 2, layerHeight: 0.6) == "p-2nozzle-0.6layer.stl")
        #expect(ExportName.name(stem: "p", ext: "3mf", nozzle: 0.25, layerHeight: 0.1 + 0.2) == "p-0.25nozzle-0.3layer.3mf")
        #expect(ExportName.name(stem: "p", ext: "3mf", nozzle: 0.004, layerHeight: 0.005) == "p-0nozzle-0.01layer.3mf")
    }

    @Test func settingsThatAreNotPositiveNumbersAreLeftOut() {
        #expect(ExportName.name(stem: "p", ext: "3mf", nozzle: 0.4, layerHeight: .nan) == "p-0.4nozzle.3mf")
        #expect(ExportName.name(stem: "p", ext: "3mf", nozzle: nil, layerHeight: nil) == "p.3mf")
        #expect(ExportName.name(stem: "p", ext: "3mf", nozzle: 0, layerHeight: -1) == "p.3mf")
        #expect(ExportName.name(stem: "p", ext: "3mf", nozzle: .infinity, layerHeight: 1e21) == "p-Infinitynozzle-1e+21layer.3mf")
    }

    @Test func toFixedRoundsTheExactValueHalfUp() {
        // JS: (0.125).toFixed(2) is "0.13" (an exact tie rounds up), (1.005).toFixed(2) is "1.00" (it is below),
        // (0.285).toFixed(2) is "0.28", (0.145).toFixed(2) is "0.14", (8.345).toFixed(2) is "8.35"
        let cases: [(Double, String)] = [
            (0.125, "0.13"), (1.005, "1"), (0.285, "0.28"), (0.145, "0.14"), (8.345, "8.35"), (0.6, "0.6"), (0.24, "0.24"),
            (0.05, "0.05"), (0.1, "0.1"), (2, "2"), (10.5, "10.5"), (0.995, "0.99"), (0.999, "1"), (99.999, "100"),
            (0.001, "0"), (-0.001, "0"), (-0.5, "-0.5"), (0.4, "0.4"), (1.0 / 3, "0.33"),
        ]
        for (v, want) in cases { #expect(ExportName.mm(v) == want, "\(v)") }
    }

    @Test func baseDropsTheMeshExtension() {
        #expect(ExportName.base(of: "bracket.stl") == "bracket")
        #expect(ExportName.base(of: "Bracket.STL") == "Bracket")
        #expect(ExportName.base(of: "a.b.3mf") == "a.b")
        #expect(ExportName.base(of: "part.step") == "part")
        #expect(ExportName.base(of: "x.obj") == "x.obj") // only the web's four extensions
        #expect(ExportName.base(of: ".stl") == "part")
        #expect(ExportName.base(of: "") == "part")
    }
}

@MainActor
@Suite("ExportModel")
struct ExportModelTests {
    func tShape() throws -> Part {
        try #require(try loadParts(path: try writeSTL("export-tests-t.stl", box(-5, 5, -5, 5, 0, 20) + box(-20, 20, -20, 20, 20, 24))).first)
    }
    func cube() throws -> Part {
        try #require(try loadParts(path: try writeSTL("export-tests-cube.stl", box(0, 20, 0, 20, 0, 20))).first)
    }

    func rig(_ part: Part, finsOn: Bool = true, panel: any SavePanel = StubPanel(), objects: Int = 1) -> (ExportModel, BuildModel) {
        let build = BuildModel(part: part)
        let model = ExportModel(build: build, finsOn: { finsOn }, source: {
            .init(partName: "bracket.stl", part: part, nozzle: 0.4, layerHeight: 0.2)
        }, objectCount: { objects }, panel: panel)
        return (model, build)
    }

    func tmp(_ name: String) -> URL { URL(fileURLWithPath: NSTemporaryDirectory() + "export-model-\(name)") }

    @Test func nothingIsOfferedUntilTheFinsAreBuilt() async throws {
        let (model, build) = rig(try tShape(), finsOn: false)
        #expect(model.availability == .init(enabled: [], note: "No fins yet: click Add fins first."))
        let on = rig(try tShape(), finsOn: true)
        #expect(on.0.availability == .init(enabled: [], note: nil))

        build.build(rotation: identity, optionsJSON: "{}")
        #expect(model.availability.enabled.isEmpty, "not while a rebuild is out")
        await build.settled()
        #expect(model.availability.enabled == Set(ExportFormat.allCases))
        #expect(model.availability.note == nil)
    }

    @Test func aPartWithNoOverhangOffersTheTwoFilesThatWriteThePartAlone() async throws {
        let (model, build) = rig(try cube())
        build.build(rotation: identity, optionsJSON: "{}")
        await build.settled()
        #expect(model.availability.enabled == [.stl, .threeMF])
        #expect(model.availability.note == "No supports on this plate: STL and 3MF write the part alone.")
    }

    @Test func fileNamesCarryTheNozzleAndLayerHeight() throws {
        let (model, _) = rig(try tShape())
        #expect(model.fileName(for: .stl) == "bracket-fins-0.4nozzle-0.2layer.stl")
        #expect(model.fileName(for: .threeMF) == "bracket-fins-0.4nozzle-0.2layer.3mf")
        #expect(model.fileName(for: .finsOnly) == "bracket-fins-only-0.4nozzle-0.2layer.stl")
    }

    @Test func theStlHoldsThePartAndTheFinsAndTheFinsOnlyFileJustTheFins() async throws {
        let part = try tShape()
        let (model, build) = rig(part)
        build.build(rotation: identity, optionsJSON: "{}")
        await build.settled()
        let result = try #require(build.latest)
        let finTris = Int(result.fins().count() + result.pad().count()) / 3

        let stl = try #require(model.write(.stl, to: tmp("a.stl")))
        let size = try #require(try FileManager.default.attributesOfItem(atPath: stl.path)[.size] as? Int)
        #expect(size == 84 + 50 * (Int(part.info().triangles) + finTris), "part + fins, merged")

        let fins = try #require(model.write(.finsOnly, to: tmp("b.stl")))
        let finsSize = try #require(try FileManager.default.attributesOfItem(atPath: fins.path)[.size] as? Int)
        #expect(finsSize == 84 + 50 * finTris)

        // the same request writes the same bytes
        let again = try #require(model.write(.stl, to: tmp("a2.stl")))
        #expect(try Data(contentsOf: stl) == Data(contentsOf: again))

        // the 3MF is a zip
        let mf = try #require(model.write(.threeMF, to: tmp("c.3mf")))
        #expect(try Data(contentsOf: mf).prefix(4) == Data([0x50, 0x4b, 0x03, 0x04]))
        #expect(model.error == nil && model.lastWritten == mf)
    }

    @Test func aPosedBuildWritesFinsOnlyInThePrintPositionLikeThePage() async throws {
        // the page writes the fins and pad in the print position whatever the pose (docs/FFI.md, "export")
        let (model, build) = rig(try tShape())
        build.build(rotation: tilt(30), optionsJSON: "{}")
        await build.settled()
        let url = try #require(model.write(.finsOnly, to: tmp("posed.stl")))
        #expect(model.error == nil)
        let size = try #require(try FileManager.default.attributesOfItem(atPath: url.path)[.size] as? Int)
        #expect(size > 84 && (size - 84) % 50 == 0)
        #expect(model.write(.stl, to: tmp("posed-ok.stl")) != nil && model.error == nil)
    }

    @Test func runAsksWhereAndDoesNothingOnCancel() async throws {
        StubPanel.asked = []
        let part = try tShape()
        let cancelled = rig(part, panel: StubPanel(answer: nil))
        cancelled.1.build(rotation: identity, optionsJSON: "{}")
        await cancelled.1.settled()
        #expect(cancelled.0.run(.threeMF) == nil && cancelled.0.lastFormat == .stl)
        #expect(StubPanel.asked.last?.0 == "bracket-fins-0.4nozzle-0.2layer.3mf" && StubPanel.asked.last?.1.preferredFilenameExtension == "3mf")

        let target = tmp("run.stl")
        let ok = rig(part, panel: StubPanel(answer: target))
        ok.1.build(rotation: identity, optionsJSON: "{}")
        await ok.1.settled()
        #expect(ok.0.run(.finsOnly) == target && ok.0.lastFormat == .finsOnly)
        // a format that is not offered never reaches the panel
        let before = StubPanel.asked.count
        let none = rig(part, panel: StubPanel(answer: target))
        #expect(none.0.run(.stl) == nil && StubPanel.asked.count == before)
    }

    @Test func aFileWithSeveralObjectsSaysOneFilePerObject() throws {
        let (several, _) = rig(try tShape(), objects: 3)
        #expect(several.multiObjectNote?.contains("3 objects") == true)
        #expect(rig(try tShape()).0.multiObjectNote == nil)
    }
}
