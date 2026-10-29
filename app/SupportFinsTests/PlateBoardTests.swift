import Foundation
import SupportFinsCore
import Testing
import Viewer

@testable import Support_Fins

/// Several models on one plate (`ui/models.js`): Add model parks the one on show and gives the new one a free spot,
/// a click swaps a parked one in with its own pose, the settings and undo are the plate's, and Remove model has an
/// Undo.
@MainActor
@Suite("Plate board")
struct PlateBoardTests {
    /// A window with a 40 mm tower on show (a stem under a slab, so it has an overhang to support).
    func window(name: String = "plates-a") async throws -> (ViewerModel, PlateBoard) {
        let model = ViewerModel()
        try #require(model.controller != nil, "no Metal device")
        await model.load(url: try stl(name))
        let board = try #require(model.features[PlatesFeature.self]?.board)
        return (model, board)
    }

    func stl(_ name: String, slab: Float = 30) throws -> URL {
        URL(fileURLWithPath: try TestParts.write(
            TestParts.box(-5, 5, -5, 5, 0, 20) + TestParts.box(-slab, slab, -slab, slab, 20, 24), name: "\(name)-\(UUID().uuidString).stl"))
    }

    func add(_ model: ViewerModel, _ name: String, slab: Float = 30) async throws {
        await model.load(url: try stl(name, slab: slab), adding: true)
        #expect(model.status == nil, "\(model.status ?? "")")
    }

    @Test func addModelParksTheOneOnShowAndStandsTheNewOneAtAFreeSpot() async throws {
        let (model, board) = try await window(name: "first")
        try await add(model, "second")
        #expect(board.plate.models.count == 2 && board.plate.live == 1 && board.modelCount == 2)
        let first = board.plate.models[0]
        #expect(first.stash?.part.info.name.hasPrefix("first") == true)
        #expect(model.session.parts.count == 1 && model.session.parts[0].info.name.hasPrefix("second"))
        // the new model stands clear of the parked one, `gap` mm apart
        let live = board.plate.liveSlot
        let a = try #require(board.footprint(of: first)).placed(x: first.xy.x, y: first.xy.y)
        let b = try #require(board.footprint(of: live)).placed(x: live.xy.x, y: live.xy.y)
        #expect(a.dilate(r: PlateBoard.gap).sharedCount(other: b) == 0, "first \(first.xy) second \(live.xy)")
        // the viewer draws the parked one, beside the live one at the origin
        let scene = try #require(model.controller?.renderer.plateScene)
        #expect(scene.parked.count == 1 && board.parkedSlots.count == 1 && board.parkedSlots[0] === first)
        #expect(scene.beds == [SIMD2(Float(-live.xy.x), Float(-live.xy.y))])
        #expect(model.session.owner == live.id)
    }

    @Test func addingToAnEmptyWindowIsAnOrdinaryLoad() async throws {
        let model = ViewerModel()
        try #require(model.controller != nil, "no Metal device")
        await model.load(url: try stl("lone"), adding: true)
        let board = try #require(model.features[PlatesFeature.self]?.board)
        #expect(board.plate.models.count == 1 && board.modelCount == 1 && model.hasPart)
    }

    @Test func aClickedModelIsEditedWithItsOwnPoseUnderThePlatesSettings() async throws {
        let (model, board) = try await window(name: "first")
        let session = model.session
        session.rotate90(about: .x)
        let turned = session.pose
        try await add(model, "second")
        #expect(session.pose == .identity, "the new model starts unturned")
        var s = session.settings
        s.tineDensity = 70
        session.setSettings(s)
        let second = board.plate.liveSlot
        board.select(0)
        #expect(board.plate.live == 0 && board.plate.models[1] === second && second.stash != nil)
        #expect(session.pose == turned, "the first model comes back as it was left")
        #expect(session.settings.tineDensity == 70, "the settings are the plate's, not a model's")
        #expect(model.session.parts[0].info.name.hasPrefix("first") && model.controller?.partCount == 1)
        // and back
        board.select(1)
        #expect(session.pose == .identity && board.plate.models[0].stash?.pose == turned)
    }

    @Test func undoIsThePlatesAndEditsTheModelAStepBelongedTo() async throws {
        let (model, board) = try await window(name: "first")
        let session = model.session
        session.rotate90(about: .x)
        let first = session.pose
        try await add(model, "second")
        session.rotate90(about: .y)
        let second = session.pose
        #expect(second != .identity)
        // the second model's turn undoes; then the first's, which brings the first model back to edit
        session.undo()
        #expect(session.pose == .identity && board.plate.live == 1)
        session.undo()
        #expect(board.plate.live == 0 && session.pose == .identity, "the first model's own turn undone")
        session.redo()
        #expect(board.plate.live == 0 && session.pose == first)
        session.redo()
        #expect(board.plate.live == 1 && session.pose == second)
    }

    @Test func aStepOfARemovedModelIsPassedOver() async throws {
        let (model, board) = try await window(name: "first")
        let session = model.session
        session.rotate90(about: .x)
        try await add(model, "second")
        session.rotate90(about: .y)
        board.select(0)
        board.removeModel()                    // the first model goes; its step stays in the history
        #expect(board.plate.models.count == 1 && session.parts[0].info.name.hasPrefix("second"))
        session.undo()                         // the second's turn
        #expect(session.pose == .identity)
        #expect(!session.canUndo || session.history.undoIndex(accepting: { $0.owner == board.plate.liveSlot.id }) == nil)
    }

    @Test func removeModelGoesOnWithTheOneBeforeAndItsToastBringsItBack() async throws {
        let (model, board) = try await window(name: "first")
        try await add(model, "second")
        let second = board.plate.liveSlot
        board.removeModel()
        #expect(board.plate.models.count == 1 && board.plate.live == 0 && model.session.parts[0].info.name.hasPrefix("first"))
        let toast = try #require(board.toast)
        #expect(toast.hasPrefix("Removed second"), "\(toast)")
        board.undoToast()
        #expect(board.toast == nil && board.plate.models.count == 2 && board.plate.liveSlot === second)
        #expect(model.session.parts[0].info.name.hasPrefix("second") && board.plate.models[0].stash != nil)
        #expect(model.controller?.renderer.plateScene.parked.count == 1)
    }

    @Test func removingTheLastModelLeavesTheWindowEmptyAndUndoPutsItBack() async throws {
        let (model, board) = try await window(name: "only")
        board.removeModel()
        #expect(!model.hasPart && model.showsEmptyState && board.modelCount == 0 && board.toast?.hasPrefix("Removed only") == true)
        #expect(model.wiring?.build == nil && model.controller?.renderer.plateScene.parked.isEmpty == true)
        board.undoToast()
        #expect(model.hasPart && board.modelCount == 1 && model.session.parts[0].info.name.hasPrefix("only"))
        // an empty plate has nothing to remove
        board.removeModel(); board.expire()
        board.removeModel()
        #expect(board.toast == nil)
    }

    @Test func aRemovalPastItsUndoIsFreed() async throws {
        let (model, board) = try await window(name: "first")
        try await add(model, "second")
        board.removeModel()
        board.expire()
        #expect(board.toast == nil)
        board.undoToast()
        #expect(board.modelCount == 1, "nothing to bring back")
    }

    @Test func parkedModelsKeepTheirFinsToThePlatesSettings() async throws {
        let (model, board) = try await window(name: "first")
        let session = model.session
        session.perform("Add Fins") { session.finsOn = true }
        let live = try #require(model.wiring?.build)
        #expect(await eventually { live.latest != nil && !live.building })
        try await add(model, "second")
        let first = try #require(board.plate.models[0].stash)
        // the first model's own build came with it, current for the plate's settings
        #expect(first.build === live && first.build.latest != nil)
        #expect(!board.isStale(first))
        let drawn = try #require(model.controller?.renderer.plateScene.parked.first)
        #expect(drawn.supports != nil && !drawn.stale)
        // a setting the engine takes: the parked fins grey, then rebuild by themselves
        var s = session.settings
        s.tineDensity = 65
        session.setSettings(s)
        board.scheduleParked()
        #expect(board.isStale(first))
        #expect(await eventually { !board.isStale(first) && !first.build.building })
        #expect(first.build.latestRequest == BuildRequest(rotation: first.pose.columnMajor, settings: session.settings))
        // fins off: a parked model keeps none
        session.perform("Fins Off") { session.finsOn = false }
        board.scheduleParked()
        #expect(await eventually { first.build.latest == nil })
        #expect(model.controller?.renderer.plateScene.parked.first?.supports == nil)
    }

    @Test func aClickOnAParkedModelEditsIt() async throws {
        let (model, board) = try await window(name: "first")
        try await add(model, "second")
        let controller = try #require(model.controller)
        // the camera looks at the live model; find the parked one's pixel and tap it
        let size = 400
        controller.renderer.camera = OrbitCamera(target: [0, 0, 10], distance: 400, azimuth: -.pi / 2, elevation: OrbitCamera.maxElevation, aspect: 1)
        let first = board.plate.models[0]
        let at = controller.renderer.camera.project([Float(first.xy.x - board.plate.liveSlot.xy.x), Float(first.xy.y - board.plate.liveSlot.xy.y), 24])
        let x = Int((at.x + 1) / 2 * Float(size)), y = Int((1 - at.y) / 2 * Float(size))
        board.tapped(x: x, y: y, width: size, height: size)
        #expect(await eventually { board.plate.live == 0 })
        #expect(model.session.parts[0].info.name.hasPrefix("first"))
    }

    @Test func theStrengthLoadAndTheSuggestListAreKeptPerModel() async throws {
        let (model, board) = try await window(name: "first")
        let strength = try #require(model.features[StrengthFeature.self])
        let suggest = try #require(model.features[SuggestFeature.self])
        try #require(strength.model != nil && suggest.model != nil)
        strength.model?.setLoad(.right)
        let load = try #require(strength.model?.loadLocal)
        suggest.model?.rank(thresholdDegrees: 45, optionsJSON: "{}", tunablesJSON: "{}", currentBore: 0, density: 1.24)
        #expect(await eventually { suggest.model?.kept != nil })
        let ranked = try #require(suggest.model?.kept)
        suggest.model?.pick(0)
        try await add(model, "second")
        // the new model has neither
        #expect(strength.model?.loadLocal == nil && suggest.model?.kept == nil)
        #expect(model.controller?.loadDirection == nil)
        board.select(0)
        #expect(strength.model?.loadLocal == load && model.controller?.loadDirection != nil)
        #expect(suggest.model?.kept?.result.rows.count == ranked.result.rows.count && suggest.model?.kept?.picked == 0)
        board.select(1)
        #expect(strength.model?.loadLocal == nil && suggest.model?.kept == nil)
    }
}
