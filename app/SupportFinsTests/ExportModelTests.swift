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

private func writeSTL(_ name: String, _ tris: [[Float]], in dir: URL) throws -> String {
    var data = Data(count: 80)
    var count = UInt32(tris.count)
    withUnsafeBytes(of: &count) { data.append(contentsOf: $0) }
    for t in tris {
        for f in [Float](repeating: 0, count: 3) + t { var x = f; withUnsafeBytes(of: &x) { data.append(contentsOf: $0) } }
        data.append(contentsOf: [0, 0])
    }
    let path = dir.appending(path: name).path(percentEncoded: false)
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
    // the engine names the files (`export_file_name`); these are crates/sf-io/tests/exportname.rs's vectors
    @Test func nameCarriesNozzleAndLayer() throws {
        #expect(try exportFileName(base: "bracket", kind: .threeMf, nozzle: 0.6, layerHeight: 0.24) == "bracket-fins-0.6nozzle-0.24layer.3mf")
        #expect(try exportFileName(base: "p", kind: .stl, nozzle: 2, layerHeight: 0.6) == "p-fins-2nozzle-0.6layer.stl")
        #expect(try exportFileName(base: "p", kind: .finsOnly, nozzle: 0.25, layerHeight: 0.1 + 0.2) == "p-fins-only-0.25nozzle-0.3layer.stl")
        #expect(try exportFileName(base: "p", kind: .threeMf, nozzle: 0.004, layerHeight: 0.005) == "p-fins-0nozzle-0.01layer.3mf")
    }

    @Test func settingsThatAreNotPositiveNumbersAreLeftOut() throws {
        #expect(try exportFileName(base: "p", kind: .threeMf, nozzle: 0.4, layerHeight: .nan) == "p-fins-0.4nozzle.3mf")
        #expect(try exportFileName(base: "p", kind: .threeMf, nozzle: 0, layerHeight: -1) == "p-fins.3mf")
    }

    @Test func baseDropsTheMeshExtension() {
        #expect(ExportModel.base(of: "bracket.stl") == "bracket")
        #expect(ExportModel.base(of: "Bracket.STL") == "Bracket")
        #expect(ExportModel.base(of: "a.b.3mf") == "a.b")
        #expect(ExportModel.base(of: "part.step") == "part")
        #expect(ExportModel.base(of: "x.obj") == "x.obj") // only the web's four extensions
        #expect(ExportModel.base(of: ".stl") == "part")
        #expect(ExportModel.base(of: "") == "part")
    }
}

@MainActor
@Suite("ExportModel")
final class ExportModelTests {
    /// This test's own directory (a suite instance is made per test): its parts and the files it exports.
    let dir: URL

    init() throws { dir = try Scratch.directory() }
    deinit { Scratch.remove(dir) }

    func tShape() throws -> Part {
        try #require(try loadParts(path: try writeSTL("export-tests-t.stl", box(-5, 5, -5, 5, 0, 20) + box(-20, 20, -20, 20, 20, 24), in: dir)).first)
    }
    func cube() throws -> Part {
        try #require(try loadParts(path: try writeSTL("export-tests-cube.stl", box(0, 20, 0, 20, 0, 20), in: dir)).first)
    }

    func rig(_ part: Part, finsOn: Bool = true, panel: any SavePanel = StubPanel(),
             pose: [Double] = identity, removed: Set<FinKey> = []) -> (ExportModel, BuildModel) {
        let build = BuildModel(part: part)
        let model = ExportModel(build: build, finsOn: { finsOn }, source: {
            .init(partName: "bracket.stl", part: part, nozzle: 0.4, layerHeight: 0.2, pose: pose, threshold: 45, bed: [256, 256])
        }, removed: { removed }, panel: panel)
        return (model, build)
    }

    func tmp(_ name: String) -> URL { dir.appending(path: name) }

    @Test func withTheFinsOffThePartAloneIsOfferedAndFinsOnlyWaitsForFins() async throws {
        let (model, build) = rig(try tShape(), finsOn: false)
        #expect(model.availability == .init(enabled: [.stl, .threeMF],
                                            note: "No fins yet: STL and 3MF write the part alone. Click Add fins first."))
        let (on, onBuild) = rig(try tShape(), finsOn: true)
        #expect(on.availability == .init(enabled: [], note: nil))

        onBuild.build(rotation: identity, optionsJSON: FinSettings().optionsJSON(), tunablesJSON: FinSettings().tunablesJSON())
        #expect(on.availability.enabled.isEmpty, "not while a rebuild is out")
        await onBuild.settled()
        #expect(on.availability.enabled == Set(ExportFormat.allCases))
        #expect(on.availability.note == nil)
        _ = build
    }

    @Test func theFinsOffWritesThePartAloneWithNoBuild() async throws {
        let part = try tShape()
        let (model, _) = rig(part, finsOn: false)
        let stl = try #require(await model.write(.stl, to: tmp("alone.stl")))
        let size = try #require(try FileManager.default.attributesOfItem(atPath: stl.path)[.size] as? Int)
        #expect(size == 84 + 50 * Int(part.info().triangles))
        let mf = try #require(await model.write(.threeMF, to: tmp("alone.3mf")))
        #expect(try Data(contentsOf: mf).prefix(4) == Data([0x50, 0x4b, 0x03, 0x04]))
        #expect(model.error == nil)
    }

    @Test func removedFinsAreLeftOut() async throws {
        let part = try tShape()
        let (plain, build) = rig(part)
        build.build(rotation: identity, optionsJSON: FinSettings().optionsJSON(), tunablesJSON: FinSettings().tunablesJSON())
        await build.settled()
        let result = try #require(build.latest)
        let keys = Set(build.keys.compactMap(\.sig))
        let gone = try #require(keys.first)
        let (cut, cutBuild) = rig(part, removed: [gone])
        cutBuild.build(rotation: identity, optionsJSON: FinSettings().optionsJSON(), tunablesJSON: FinSettings().tunablesJSON())
        await cutBuild.settled()
        let a = try #require(await plain.write(.finsOnly, to: tmp("all-fins.stl")))
        let b = try #require(await cut.write(.finsOnly, to: tmp("cut-fins.stl")))
        let sa = try #require(try FileManager.default.attributesOfItem(atPath: a.path)[.size] as? Int)
        let sb = try #require(try FileManager.default.attributesOfItem(atPath: b.path)[.size] as? Int)
        #expect(sb < sa, "a fin fewer (\(result.fins().count()) fin vertices before)")
    }

    @Test func aPartWithNoOverhangOffersTheTwoFilesThatWriteThePartAlone() async throws {
        let (model, build) = rig(try cube())
        build.build(rotation: identity, optionsJSON: FinSettings().optionsJSON(), tunablesJSON: FinSettings().tunablesJSON())
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
        build.build(rotation: identity, optionsJSON: FinSettings().optionsJSON(), tunablesJSON: FinSettings().tunablesJSON())
        await build.settled()
        let result = try #require(build.latest)
        let finTris = Int(result.fins().count() + result.pad().count()) / 3

        let stl = try #require(await model.write(.stl, to: tmp("a.stl")))
        let size = try #require(try FileManager.default.attributesOfItem(atPath: stl.path)[.size] as? Int)
        #expect(size == 84 + 50 * (Int(part.info().triangles) + finTris), "part + fins, merged")

        let fins = try #require(await model.write(.finsOnly, to: tmp("b.stl")))
        let finsSize = try #require(try FileManager.default.attributesOfItem(atPath: fins.path)[.size] as? Int)
        #expect(finsSize == 84 + 50 * finTris)

        // the same request writes the same bytes
        let again = try #require(await model.write(.stl, to: tmp("a2.stl")))
        #expect(try Data(contentsOf: stl) == Data(contentsOf: again))

        // the 3MF is a zip
        let mf = try #require(await model.write(.threeMF, to: tmp("c.3mf")))
        #expect(try Data(contentsOf: mf).prefix(4) == Data([0x50, 0x4b, 0x03, 0x04]))
        #expect(model.error == nil && model.lastWritten == mf)
    }

    @Test func aPosedBuildWritesFinsOnlyInThePrintPositionLikeThePage() async throws {
        // the page writes the fins and pad in the print position whatever the pose (docs/FFI.md, "export")
        let (model, build) = rig(try tShape())
        build.build(rotation: tilt(30), optionsJSON: FinSettings().optionsJSON(), tunablesJSON: FinSettings().tunablesJSON())
        await build.settled()
        let url = try #require(await model.write(.finsOnly, to: tmp("posed.stl")))
        #expect(model.error == nil)
        let size = try #require(try FileManager.default.attributesOfItem(atPath: url.path)[.size] as? Int)
        #expect(size > 84 && (size - 84) % 50 == 0)
        #expect(await model.write(.stl, to: tmp("posed-ok.stl")) != nil && model.error == nil)
    }

    @Test func runAsksWhereAndDoesNothingOnCancel() async throws {
        StubPanel.asked = []
        let part = try tShape()
        let cancelled = rig(part, panel: StubPanel(answer: nil))
        cancelled.1.build(rotation: identity, optionsJSON: FinSettings().optionsJSON(), tunablesJSON: FinSettings().tunablesJSON())
        await cancelled.1.settled()
        #expect(await cancelled.0.run(.threeMF) == nil && cancelled.0.lastFormat == .stl)
        #expect(StubPanel.asked.last?.0 == "bracket-fins-0.4nozzle-0.2layer.3mf" && StubPanel.asked.last?.1.preferredFilenameExtension == "3mf")

        let target = tmp("run.stl")
        let ok = rig(part, panel: StubPanel(answer: target))
        ok.1.build(rotation: identity, optionsJSON: FinSettings().optionsJSON(), tunablesJSON: FinSettings().tunablesJSON())
        await ok.1.settled()
        #expect(await ok.0.run(.finsOnly) == target && ok.0.lastFormat == .finsOnly)
        // a format that is not offered never reaches the panel
        let before = StubPanel.asked.count
        let none = rig(part, panel: StubPanel(answer: target))
        #expect(await none.0.run(.stl) == nil && StubPanel.asked.count == before)
    }

    /// S-10: a NaN pose made `JSONSerialization.data(withJSONObject:)` raise an ObjC exception that `try?` does not catch.
    @Test func aNonFinitePoseOrThresholdGivesNoOptionsInsteadOfCrashing() throws {
        let part = try cube()
        for bad in [Double.nan, .infinity] {
            var pose = identity; pose[4] = bad
            let (a, _) = rig(part, finsOn: false, pose: pose)
            #expect(a.optionsJSON(for: .stl) == nil)
            let build = BuildModel(part: part)
            let b = ExportModel(build: build, finsOn: { false }, source: {
                .init(partName: "p.stl", part: part, nozzle: 0.4, layerHeight: 0.2, pose: identity, threshold: bad, bed: [256, 256])
            }, removed: { [] }, panel: StubPanel())
            #expect(b.optionsJSON(for: .threeMF) == nil)
        }
        let (ok, _) = rig(part, finsOn: false)
        #expect(ok.optionsJSON(for: .stl) != nil)
    }
}
