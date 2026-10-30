import Foundation
import SupportFinsCore
import Testing
import UniformTypeIdentifiers
import Viewer

@testable import Support_Fins

/// Models that collide and Arrange (`ui/arrange.js`), and the plate exports (`ui/export.js`, `ui/plates.js`).
@MainActor
@Suite("Plate clash, Arrange and exports")
struct PlateExportTests {
    let helper = PlateBoardTests()

    struct Panel: SavePanel {
        var url: URL
        var asked: Box = Box()
        final class Box { var names: [String] = [] }
        func url(suggesting name: String, type: UTType) -> URL? { asked.names.append(name); return url }
    }

    func two(fins: Bool = false) async throws -> (ViewerModel, PlateBoard) {
        let (model, board) = try await helper.window(name: "first")
        if fins {
            model.session.perform("Add Fins") { model.session.finsOn = true }
            let b = try #require(model.wiring?.build)
            #expect(await eventually { b.latest != nil && !b.building })
        }
        try await helper.add(model, "second", slab: 20)
        return (model, board)
    }

    /// Put the live model right on top of the parked one.
    func overlap(_ board: PlateBoard) {
        board.placeLive(board.plate.liveSlot, board.plate.models[0].xy)
        board.sync()
    }

    func dir() throws -> URL {
        let d = FileManager.default.temporaryDirectory.appending(path: "plate-export-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        return d
    }

    @Test func modelsThatShareACellShowRedAndTheBannerOffersArrange() async throws {
        let (model, board) = try await two()
        #expect(!board.clashing && model.controller?.renderer.plateScene.clash.isEmpty == true)
        overlap(board)
        #expect(board.clashing)
        let cells = try #require(model.controller?.renderer.plateScene.clash)
        #expect(!cells.isEmpty)
    }

    @Test func arrangePacksTheModelsClearAndIsOneUndoStep() async throws {
        let (model, board) = try await two()
        overlap(board)
        let before = board.plate.models.map(\.xy)
        await board.arrange()
        #expect(!board.clashing, "arranged apart")
        let after = board.plate.models.map(\.xy)
        #expect(after != before)
        // the biggest takes the middle, and the others stand on the bed, `gap` clear
        let a = try #require(board.footprint(of: board.plate.models[0])).placed(x: after[0].x, y: after[0].y)
        let b = try #require(board.footprint(of: board.plate.models[1])).placed(x: after[1].x, y: after[1].y)
        #expect(try a.dilate(r: PlateBoard.gap).sharedCount(other: b) == 0)
        // one undo step puts them back, and redo packs them again
        model.session.undo()
        #expect(board.plate.models.map(\.xy) == before && board.clashing)
        model.session.redo()
        #expect(board.plate.models.map(\.xy) == after && !board.clashing)
        // nothing to move, no step
        await board.arrange()
        model.session.undo()
        #expect(board.plate.models.map(\.xy) == before)
    }

    @Test func arrangeDoesNothingWithOneModelOrWhileDragging() async throws {
        let (model, board) = try await helper.window()
        await board.arrange()
        #expect(!model.session.canUndo)
        try await helper.add(model, "second")
        overlap(board)
        model.session.dragLocked = true
        await board.arrange()
        #expect(board.clashing)
        model.session.dragLocked = false
    }

    @Test func aPlatesThreeMFHoldsEveryModelAtItsSpotAndIsNamedAfterThem() async throws {
        let (model, board) = try await two(fins: true)
        let w = try #require(board.plateWrite(0))
        #expect(w.models.count == 2 && w.models[0].build != nil)
        #expect(w.fileName.hasPrefix("first") && w.fileName.contains("-and-1-more-fins-") && w.fileName.hasSuffix(".3mf"), Comment(rawValue: w.fileName))
        let out = try dir()
        let panel = Panel(url: out.appending(path: w.fileName))
        board.savePanel = panel
        #expect(await eventually { !model.wiring!.build!.building })
        let url = try #require(await board.exportPlate(), "\(board.exportError ?? "")")
        let text = String(decoding: try Data(contentsOf: url), as: UTF8.self)
        #expect(text.contains("first") && text.contains("second"), "both models are objects of the file")
        #expect(panel.asked.names == [w.fileName])
        // a plate of one keeps the model's own name
        let single = ViewerModel()
        await single.load(url: try helper.stl("alone"))
        let sb = try #require(single.features[PlatesFeature.self]?.board)
        let single3mf = try #require(sb.plateWrite(0)).fileName
        #expect(single3mf.hasPrefix("alone") && single3mf.contains("-fins-") && !single3mf.contains("more"))
    }

    @Test func finsOffWritesThePartsAloneAndRemovedFinsStayOut() async throws {
        let (model, board) = try await two(fins: true)
        let live = try #require(model.wiring?.build)
        let all = try #require(board.plateWrite(0)).models
        #expect(all[1].removed.isEmpty)
        // a fin removed on the live model is left out of its objects
        if let fin = live.keys.first?.sig {
            model.session.perform("Remove Fin") { model.session.removedFins = [fin] }
            #expect(try #require(board.plateWrite(0)).models[1].removed.count == 1)
        }
        model.session.perform("Fins Off") { model.session.finsOn = false }
        let off = try #require(board.plateWrite(0)).models
        #expect(off.allSatisfy { $0.build == nil && $0.removed.isEmpty })
        let out = try dir().appending(path: "off.3mf")
        let w = try #require(board.plateWrite(0))
        #expect(await board.writePlate(w, to: out) == out && board.exportError == nil)
    }

    @Test func allPlatesIsOneZipOfEachPlatesThreeMF() async throws {
        let (model, board) = try await two(fins: true)
        board.addPlate()
        await model.load(url: try helper.stl("third", slab: 15))
        model.session.perform("Add Fins") { model.session.finsOn = true }
        #expect(await eventually { model.wiring?.build?.latest != nil && model.wiring?.build?.building == false })
        let out = try dir()
        let panel = Panel(url: out.appending(path: "support-fins-plates.zip"))
        board.savePanel = panel
        let url = try #require(await board.exportAll(), "\(board.exportError ?? "")")
        #expect(panel.asked.names == ["support-fins-plates.zip"])
        let text = String(decoding: try Data(contentsOf: url), as: UTF8.self)
        #expect(text.contains("plate-1-first") && text.contains("-and-1-more-fins-") && text.contains("plate-2-third"), "two files in the zip")
        // an empty plate is not in it
        board.addPlate()
        let again = try #require(await board.exportAll())
        let zipped = String(decoding: try Data(contentsOf: again), as: UTF8.self)
        #expect(!zipped.contains("plate-3-"))
    }

    /// Audit X-08: a model named like a path (the file chooses the name) must not put that path in the zip.
    @Test func aTraversalNamedModelExportsASafeZip() async throws {
        let model = ViewerModel()
        try #require(model.controller != nil, "no Metal device")
        let evil = FileManager.default.temporaryDirectory.appending(path: "evil-\(UUID().uuidString).3mf")
        try TestThreeMF.box(named: "x/../../../../Library/LaunchAgents/evil").write(to: evil)
        await model.load(url: evil)
        let board = try #require(model.features[PlatesFeature.self]?.board)
        #expect(model.session.parts.first?.info.name.contains("../") == true, "the part carries the file's name")
        try await helper.add(model, "second", slab: 20)
        let out = try dir()
        board.savePanel = Panel(url: out.appending(path: "support-fins-plates.zip"))
        board.addPlate()
        await model.load(url: evil)
        let url = try #require(await board.exportAll(), "\(board.exportError ?? "")")
        let zipped = String(decoding: try Data(contentsOf: url), as: UTF8.self)
        #expect(zipped.contains("plate-1-x") && zipped.contains("plate-2-x"), "both plates are in the zip")
        #expect(!zipped.contains("../") && !zipped.contains("LaunchAgents/"), "no path in an entry name")
    }

    @Test func onePlateWithModelsIsTheThreeMFAlone() async throws {
        let (model, board) = try await two()
        defer { withExtendedLifetime(model) {} }         // the board works through its window
        board.addPlate()
        let out = try dir().appending(path: "x.3mf")
        let panel = Panel(url: out)
        board.savePanel = panel
        // the first plate only has models: its 3MF alone, named plate-1-...
        let url = try #require(await board.exportAll())
        #expect(panel.asked.names.first?.hasPrefix("plate-1-first") == true && panel.asked.names.first?.contains("-and-1-more-fins-") == true)
        let head = try Data(contentsOf: url).prefix(2)
        #expect(Array(head) == [0x50, 0x4b])
    }

    @Test func theExportMenuOffersThePlateExportsAndArrangeFirst() async throws {
        let (model, board) = try await two()
        let entries = ExportFeature.itemEntries(model.wiring?.exporter, board: board).flatMap(\.snapshot)
        #expect(entries.contains("STL") && entries.contains("3MF") && !entries.contains("All plates (3MF)") && !entries.contains("Arrange first"))
        overlap(board)
        let clash = ExportFeature.itemEntries(model.wiring?.exporter, board: board).flatMap(\.snapshot)
        #expect(Array(clash.prefix(2)) == ["(Models overlap: the 3MF prints them into each other.)", "Arrange first"])
        board.addPlate()
        await model.load(url: try helper.stl("third"))
        let many = ExportFeature.itemEntries(model.wiring?.exporter, board: board).flatMap(\.snapshot)
        #expect(many.contains("All plates (3MF)"))
    }

    @Test func aSecondPlatesFinsAreBuiltWhereItsModelsAreParkedBeforeItIsWritten() async throws {
        let (model, board) = try await two(fins: true)
        board.addPlate()
        await model.load(url: try helper.stl("third", slab: 15))
        let stale = try #require(board.plates[0].models.first { $0.stash != nil }?.stash)
        // changed on this plate: the first plate's fins are for the settings it was left with
        var s = model.session.settings
        s.tineDensity = 55
        model.session.setSettings(s)
        _ = await board.settlePlate(0)
        let want = BuildRequest(rotation: stale.pose.columnMajor, settings: board.plateSettings(0).settings)
        #expect(stale.build.requested == want && !stale.build.building)
    }

    @Test func theToolbarsArrangeIsThereWithSeveralModelsAndIsOneUndoStep() async throws {
        let (model, board) = try await helper.window(name: "first")
        let registry = model.toolbar
        // one model: no button (`syncButton`)
        #expect(registry.slots.contains(.arrange) && registry.badges[.arrange]?.isShown() == false)
        try await helper.add(model, "second", slab: 20)
        #expect(registry.badges[.arrange]?.isShown() == true && registry.badges[.arrange]?.isEnabled() == true)
        // the slot sits between Move and Rotate
        let ids = registry.slots.map(\.id)
        #expect(ids.firstIndex(of: "move")! < ids.firstIndex(of: "arrange")! && ids.firstIndex(of: "arrange")! < ids.firstIndex(of: "rotate")!)
        overlap(board)
        let before = board.plate.models.map(\.xy)
        registry.toggle(.arrange)
        #expect(await eventually { !board.clashing })
        let after = board.plate.models.map(\.xy)
        #expect(after != before)
        model.session.undo()
        #expect(board.plate.models.map(\.xy) == before, "one undo step puts them back")
        // the last model off: no button again
        board.removeModel()
        #expect(registry.badges[.arrange]?.isShown() == false)
    }

    /// A model's hand-placed walls are written with its fins and pad in a plate's 3MF, the live model's and a parked
    /// one's alike (`plateGeometry` takes `activeAdded()`'s walls; a parked model's are stood for its pose, `ui/parkedbuild.js`).
    @Test func aPlatesThreeMFCarriesAParkedModelsDrawnWall() async throws {
        let (model, board) = try await helper.window(name: "first")
        model.session.perform("Add Fins") { model.session.finsOn = true }
        let build = try #require(model.wiring?.build)
        #expect(await eventually { build.latest != nil && !build.building })
        let draw = try #require(model.features[DrawFeature.self]).model
        // a wall across the slab's underside (the part stands where the file has it: its own frame is print space)
        model.session.perform("Draw Wall") { model.session.drawnWalls = [DrawnWall.wall(from: [-20, -20, 20], to: [20, -20, 20])] }
        draw.sync()
        #expect(await eventually { draw.sweepCurrent })
        let live = try #require(board.plateWrite(0)).models
        #expect(live.count == 1 && live[0].drawn != nil, "the live model's wall")
        // parked: the second model is live, the first keeps its wall
        try await helper.add(model, "second", slab: 20)
        #expect(model.session.drawnWalls.isEmpty, "the new model has none")
        #expect(await eventually { model.wiring?.build?.latest != nil && model.wiring?.build?.building == false })
        let w = try #require(board.plateWrite(0))
        try #require(w.models.count == 2)
        let parked = try #require(w.models[0].drawn, "the parked model's wall is written")
        #expect(parked.triangles().count() > 0)
        #expect(w.models[1].drawn == nil)
        // the file holds it: the same plate without the wall is smaller
        let out = try dir()
        let with = try #require(await board.writePlate(w, to: out.appending(path: "with.3mf")), "\(board.exportError ?? "")")
        var bare = w
        bare.models[0].drawn = nil
        let without = try #require(await board.writePlate(bare, to: out.appending(path: "without.3mf")))
        let a = try Data(contentsOf: with).count, b = try Data(contentsOf: without).count
        #expect(a > b, "with the wall \(a) bytes, without \(b)")
        // the export stands the walls off the main thread: the deferred write holds a recipe, and what it writes is the same file
        let lazy = try #require(board.plateWrite(0, deferStanding: true))
        #expect(lazy.resolved().models[0].drawn != nil)
        let lazyFile = try #require(await board.writePlate(lazy, to: out.appending(path: "lazy.3mf")), "\(board.exportError ?? "")")
        let eager = try #require(await board.writePlate(w, to: out.appending(path: "eager.3mf")))
        #expect(try Data(contentsOf: lazyFile) == Data(contentsOf: eager), "byte-identical")
    }
}
