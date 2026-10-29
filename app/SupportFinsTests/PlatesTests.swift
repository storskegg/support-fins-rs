import Foundation
import SupportFinsCore
import Testing
import Viewer

@testable import Support_Fins

/// Plates (`ui/plates.js`): several in one window, each with its models, settings and undo; the tabs' numbers, the row
/// in the scene, a click on another plate, delete and move with their Undo, several files one to a plate.
@MainActor
@Suite("Plates")
struct PlatesTests {
    let helper = PlateBoardTests()

    func window(_ name: String = "first") async throws -> (ViewerModel, PlateBoard) { try await helper.window(name: name) }

    /// A second plate holding a model of its own.
    func secondPlate(_ model: ViewerModel, _ board: PlateBoard, _ name: String = "second") async throws {
        board.addPlate()
        await model.load(url: try helper.stl(name, slab: 20))
        #expect(model.status == nil)
    }

    @Test func aNewPlateIsEmptyAndTheOldOneStaysParkedInTheRow() async throws {
        let (model, board) = try await window()
        board.addPlate()
        #expect(board.plates.count == 2 && board.active == 1 && !board.hasLive && model.showsEmptyState)
        #expect(board.tabName(0).hasPrefix("first") && board.tabName(1) == "" && board.tabCount(0) == 1 && board.tabCount(1) == 0)
        let scene = try #require(model.controller?.renderer.plateScene)
        #expect(scene.beds.count == 2 && scene.edited == 1 && scene.parked.count == 1)
        let pitch = Float(board.pitch)
        #expect(scene.beds[1] == .zero && scene.beds[0] == [-pitch, 0])
        #expect(PlateBoard.pitch(volumeX: 250) == 250 + 30)
        #expect(PlateBoard.pitch(volumeX: 100) == 120)
        #expect(PlateBoard.stem("bracket.STL") == "bracket" && PlateBoard.stem("a.b.3mf") == "a.b")
    }

    @Test func eachPlateKeepsItsSettingsAndUndo() async throws {
        let (model, board) = try await window()
        let s1 = model.session
        s1.rotate90(about: .x)
        var set = s1.settings
        set.tineDensity = 70
        s1.setSettings(set)
        try await secondPlate(model, board)
        #expect(model.session.parts[0].info.name.hasPrefix("second"))
        #expect(!s1.canUndo, "the new plate's undo starts empty")
        set.tineDensity = 40
        s1.setSettings(set)
        board.switchPlate(0)
        #expect(board.active == 0 && s1.settings.tineDensity == 70 && s1.pose != .identity)
        #expect(s1.canUndo, "the first plate's undo came back with it")
        board.switchPlate(1)
        #expect(s1.settings.tineDensity == 40 && s1.canUndo)
    }

    @Test func aClickOnAnotherPlatesBedOrModelSwitchesToIt() async throws {
        let (model, board) = try await window()
        try await secondPlate(model, board)
        let c = try #require(model.controller)
        // the first plate stands one pitch to the left of the one being edited
        let live = board.plate.liveSlot.xy
        #expect(board.plate(at: SIMD2(Float(-board.pitch - live.x) + 10, Float(-live.y) + 5)) == 0)
        #expect(board.plate(at: SIMD2(Float(-live.x), Float(-live.y))) == 1, "the edited plate's own bed")
        #expect(board.plate(at: SIMD2(Float(-live.x) + 3000, 0)) == nil, "past the row")
        // a click on the parked model of the first plate
        let size = 400
        c.renderer.camera = OrbitCamera(target: [-Float(board.pitch), 0, 10], distance: 500, azimuth: -.pi / 2,
                                        elevation: OrbitCamera.maxElevation, aspect: 1)
        let slot = board.plates[0].liveSlot
        let n = c.renderer.camera.project([Float(-board.pitch + slot.xy.x - live.x), Float(slot.xy.y - live.y), 24])
        board.tapped(x: Int((n.x + 1) / 2 * Float(size)), y: Int((1 - n.y) / 2 * Float(size)), width: size, height: size)
        #expect(await eventually { board.active == 0 })
        #expect(model.session.parts[0].info.name.hasPrefix("first"))
    }

    @Test func deletingAPlateHasAnUndoThatBringsItBackWithItsSettingsAndHistory() async throws {
        let (model, board) = try await window()
        let session = model.session
        session.rotate90(about: .x)
        let turned = session.pose
        var set = session.settings
        set.tineDensity = 77
        session.setSettings(set)
        try await secondPlate(model, board)
        set.tineDensity = 33
        session.setSettings(set)
        board.deletePlate(0)
        #expect(board.plates.count == 1 && board.active == 0 && board.toast == "Deleted plate 1")
        #expect(session.parts[0].info.name.hasPrefix("second"))
        board.undoToast()
        #expect(board.plates.count == 2 && board.active == 0 && board.toast == nil)
        #expect(session.pose == turned && session.settings.tineDensity == 77 && session.canUndo)
        #expect(board.tabName(1).hasPrefix("second"))
    }

    @Test func deletingTheOnlyPlateEmptiesItAndUndoPutsItsModelsBack() async throws {
        let (model, board) = try await window("only")
        board.deletePlate(0)
        #expect(board.plates.count == 1 && !board.hasLive && board.toast?.hasPrefix("Removed only") == true)
        board.undoToast()
        #expect(board.hasLive && model.session.parts[0].info.name.hasPrefix("only"))
        // an empty plate has nothing to delete or bring back
        board.deletePlate(0); board.expire(); board.deletePlate(0)
        #expect(board.toast == nil)
    }

    @Test func deletingAPlateOtherThanTheEditedOneKeepsTheEditedOne() async throws {
        let (model, board) = try await window()
        try await secondPlate(model, board)
        board.deletePlate(0)
        #expect(board.active == 0 && board.plates.count == 1 && model.session.parts[0].info.name.hasPrefix("second"))
        board.undoToast()
        // the restored plate is the one edited (`undoDelete` shows it)
        #expect(board.active == 0 && model.session.parts[0].info.name.hasPrefix("first") && board.tabName(1).hasPrefix("second"))
    }

    @Test func aModelMovedToAnotherPlateTakesThatPlatesSettingsAndHasAnUndo() async throws {
        let (model, board) = try await window()
        try await secondPlate(model, board)
        let m = board.plate.liveSlot
        let moved = board.moveToPlate(m, to: 0, at: [12, -7], back: [0, 0])
        #expect(moved && board.active == 0 && board.plates[1].models.count == 1 && !board.plates[1].liveSlot.stash.isSome)
        #expect(board.plate.models.count == 2 && board.plate.live == 1 && m.xy == SIMD2(12, -7) || board.plate.models.contains { $0 === m })
        let toast = try #require(board.toast)
        #expect(toast.hasPrefix("Moved second") && toast.hasSuffix("to plate 1: it takes that plate's settings"), "\(toast)")
        #expect(model.session.parts[0].info.name.hasPrefix("second"))
        board.undoToast()
        #expect(board.active == 1 && board.toast == nil && model.session.parts[0].info.name.hasPrefix("second"))
        #expect(board.plates[0].models.count == 1)
        // the same plate, or a plate that is not there, is no move
        #expect(!board.moveToPlate(board.plate.liveSlot, to: 1, at: .zero, back: nil))
        #expect(!board.moveToPlate(board.plate.liveSlot, to: 5, at: .zero, back: nil))
    }

    @Test func severalFilesGoOneToAPlate() async throws {
        let model = ViewerModel()
        try #require(model.controller != nil, "no Metal device")
        let plates = try #require(model.features[PlatesFeature.self])
        let board = plates.board
        plates.loadFiles([try helper.stl("one"), try helper.stl("two", slab: 20), try helper.stl("three", slab: 15)])
        #expect(await eventually { board.plates.count == 3 && board.tabCount(2) == 1 && model.hasPart && !model.isLoading })
        #expect(board.tabName(0).hasPrefix("one") && board.tabName(1).hasPrefix("two") && board.tabName(2).hasPrefix("three"))
        #expect(board.active == 2)
    }

    @Test func seeAllFramesTheRow() async throws {
        let (model, board) = try await window()
        try await secondPlate(model, board)
        let c = try #require(model.controller)
        board.frameAll()
        try await Task.sleep(for: .milliseconds(700))
        let live = board.plate.liveSlot.xy
        let expected = Float(0.5 - 1) * Float(board.pitch) - Float(live.x)
        #expect(abs(c.renderer.camera.target.x - expected) < 0.5 && c.renderer.camera.target.z == 0)
    }

    @Test func aPlateSwitchWhileEmptyAndTheLastEmptyPlateAreDropped() async throws {
        let (_, board) = try await window()
        board.addPlate()
        board.freePlate()
        #expect(board.plates.count == 2, "an empty plate is not added to")
        board.dropEmptyPlate()
        #expect(board.plates.count == 1 && board.active == 0)
    }
}

private extension Optional {
    var isSome: Bool { self != nil }
}
