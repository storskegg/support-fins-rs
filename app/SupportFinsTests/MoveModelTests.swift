import AppKit
import Foundation
import SupportFinsCore
import Testing
import Viewer
import simd

@testable import Support_Fins

/// Move (`web/ui/move.js`) through the window's pieces: the tool's callbacks as the viewer makes them, the slots'
/// spots, the clamp against the other models, and the one undo step a drag leaves.
@MainActor
@Suite("Move")
struct MoveModelTests {
    func stl(_ name: String) throws -> URL {
        URL(fileURLWithPath: try TestParts.write(TestParts.box(-10, 10, -10, 10, 0, 20), name: "\(name)-\(UUID().uuidString).stl"))
    }

    /// Two 20 mm blocks on one plate: the first parked, the second live (the later one is added at a free spot).
    func twoModels() async throws -> (ViewerModel, PlateBoard, MoveModel, MoveTool) {
        let host = ViewerModel()
        try #require(host.controller != nil, "no Metal device")
        await host.load(url: try stl("move-a"))
        await host.load(url: try stl("move-b"), adding: true)
        let board = try #require(host.features[PlatesFeature.self]?.board)
        let move = try #require(host.features[MoveFeature.self]?.model)
        let tool = try #require(host.controller?.move)
        #expect(board.plate.models.count == 2)
        return (host, board, move, tool)
    }

    /// One drag as the viewer feeds it: the press on `pick` at world `from`, the pointer to `to`, the button up.
    func drag(_ tool: MoveTool, _ pick: ModelPick, from: SIMD2<Double>, to: SIMD2<Double>, release: Bool = true) {
        #expect(tool.onBegin?(pick, from) == true)
        tool.onDrag?(to)
        if release { tool.onEnd?() }
    }

    @Test func moveNeedsAModelAndTurnsOnAndOff() async throws {
        let empty = ViewerModel()
        let none = try #require(empty.features[MoveFeature.self]?.model)
        none.setOn(true)
        #expect(!none.isOn, "nothing to move")
        let (host, _, move, tool) = try await twoModels()
        defer { withExtendedLifetime(host) {} }
        move.setOn(true)
        #expect(move.isOn && tool.isOn)
        move.toggle()
        #expect(!move.isOn)
        host.toolbar.toggle(.move)                    // the badge is a toggle: no popover
        #expect(move.isOn && host.toolbar.open == nil)
        #expect(host.toolbar.badges[.move]?.isLit() == true)
        host.toolbar.toggle(.move)
        #expect(!move.isOn)
    }

    @Test func draggingTheLiveModelMovesItsSlotAndShiftsTheBedAndTheCamera() async throws {
        let (host, board, move, tool) = try await twoModels()
        move.setOn(true)
        let live = board.plate.liveSlot, from = live.xy
        let camera = try #require(host.controller?.renderer.camera.target)
        // far from the parked one, so the clamp does not touch it
        drag(tool, .live, from: .zero, to: SIMD2(0, 60))
        #expect(live.xy == SIMD2(from.x, from.y + 60), "\(from) -> \(live.xy)")
        // the world went the other way and the camera with it, so on screen the bed stays put
        let scene = try #require(host.controller?.renderer.plateScene)
        #expect(scene.beds == [SIMD2(Float(-live.xy.x), Float(-live.xy.y))])
        #expect(abs((host.controller?.renderer.camera.target.y ?? 0) - (camera.y - 60)) < 1e-4)
    }

    @Test func aDragUnderHalfAMillimetreDoesNotMoveAndLeavesNoStep() async throws {
        let (host, board, move, tool) = try await twoModels()
        defer { withExtendedLifetime(host) {} }
        move.setOn(true)
        let live = board.plate.liveSlot, from = live.xy
        let steps = host.session.history.undoStack.count
        drag(tool, .live, from: .zero, to: SIMD2(0.3, 0.2))
        #expect(live.xy == from && host.session.history.undoStack.count == steps)
    }

    @Test func aModelNeverMovesIntoAnotherItStopsAtItsEdge() async throws {
        let (host, board, move, tool) = try await twoModels()
        defer { withExtendedLifetime(host) {} }
        move.setOn(true)
        let live = board.plate.liveSlot, other = board.plate.models[0]
        // drag the live block right onto the parked one
        drag(tool, .live, from: .zero, to: SIMD2(other.xy.x - live.xy.x, other.xy.y - live.xy.y))
        let a = try #require(board.footprint(of: live)).placed(x: live.xy.x, y: live.xy.y)
        let b = try #require(board.footprint(of: other)).placed(x: other.xy.x, y: other.xy.y)
        #expect(try a.sharedCount(other: b) == 0, "no shared cell: \(live.xy) vs \(other.xy)")
        #expect(live.xy != other.xy)
    }

    @Test func aParkedModelCanBeDraggedToo() async throws {
        let (host, board, move, tool) = try await twoModels()
        defer { withExtendedLifetime(host) {} }
        move.setOn(true)
        let parked = board.plate.models[0], from = parked.xy
        let press = SIMD2(from.x - board.plate.liveSlot.xy.x, from.y - board.plate.liveSlot.xy.y)
        drag(tool, .parked(0), from: press, to: press + SIMD2(0, 40))      // away from the live one, which stands below it
        #expect(parked.xy == SIMD2(from.x, from.y + 40), "from \(from) to \(parked.xy), live \(board.plate.liveSlot.xy)")
        #expect(board.plate.liveSlot.stash == nil, "the live model is still the live one")
    }

    @Test func eachDragIsOneUndoStepThatPutsTheSlotBackAndLeavesThePose() async throws {
        let (host, board, move, tool) = try await twoModels()
        move.setOn(true)
        let live = board.plate.liveSlot, from = live.xy, pose = host.session.pose
        drag(tool, .live, from: .zero, to: SIMD2(0, 50))
        let moved = live.xy
        #expect(host.session.canUndo)
        host.session.undo()
        #expect(live.xy == from && host.session.pose == pose)
        #expect(host.session.canRedo)
        host.session.redo()
        #expect(live.xy == moved)
        // the step is one for the whole drag: several moves of the pointer, one undo
        drag(tool, .live, from: .zero, to: SIMD2(30, 0), release: false)
        tool.onDrag?(SIMD2(35, 5))
        tool.onEnd?()
        host.session.undo()
        #expect(live.xy == moved)
    }

    @Test func undoAndRedoWaitForTheDropAndMoveStepsNameTheModel() async throws {
        let (host, board, move, tool) = try await twoModels()
        move.setOn(true)
        drag(tool, .live, from: .zero, to: SIMD2(0, 50))
        #expect(tool.onBegin?(.live, .zero) == true)
        #expect(!host.session.canUndo, "a model is being dragged: Undo waits for the drop")
        tool.onEnd?()
        #expect(host.session.canUndo)
        let step = try #require(host.session.history.undoStack.last)
        #expect(step.moves.count == 1 && step.moves[0].who == board.plate.liveSlot.id)
    }

    @Test func aMoveOfAModelRemovedFromThePlateIsPassedOver() async throws {
        let (host, board, move, tool) = try await twoModels()
        move.setOn(true)
        let first = board.plate.models[0]
        drag(tool, .parked(0), from: .zero, to: SIMD2(0, -40))
        // an ordinary step on the live model, after it
        host.session.rotate90(about: .z)
        host.session.undo()                          // the turn
        #expect(host.session.canUndo)
        // the moved model goes off the plate: its step cannot be taken
        board.select(0)
        board.removeModel()
        #expect(!board.plate.models.contains { $0 === first })
        let pose = host.session.pose
        host.session.undo()
        #expect(host.session.pose == pose, "the move of a removed model is passed over")
    }

    @Test func theMenuItemAndTheEscCascadeTurnItOff() async throws {
        let (host, _, move, _) = try await twoModels()
        let feature = try #require(host.features[MoveFeature.self])
        let sections = MoveFeature.menus(feature)
        guard case .toggle(let item) = sections[0].entries[1] else { Issue.record("no toggle"); return }
        #expect(item.title == "Move" && item.enabled)
        item.isOn.wrappedValue = true
        #expect(move.isOn)
        #expect(host.features.escape())
        #expect(!move.isOn)
    }

    /// Esc with Move on and a popover open folds the popover and leaves Move on (`ui/move.js:190-193`), whichever
    /// key monitor sees the Esc first: Move's (the popover is open) or the toolbar's (this Esc just folded it).
    /// The next Esc, with nothing open, leaves Move.
    @Test func escWithAPopoverOpenFoldsItAndLeavesMoveOn() async throws {
        let (host, _, move, _) = try await twoModels()
        let feature = try #require(host.features[MoveFeature.self])
        func esc(_ t: TimeInterval) throws -> NSEvent {
            try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: t, windowNumber: 0,
                                          context: nil, characters: "\u{1b}", charactersIgnoringModifiers: "\u{1b}",
                                          isARepeat: false, keyCode: 53))
        }
        move.setOn(true)
        // Move's monitor first: the popover is open, so the key is the popover's
        host.toolbar.toggle(.rotate)
        let first = try esc(10)
        #expect(!feature.escape(first))
        host.toolbar.fold(byEscape: first)
        #expect(move.isOn && host.toolbar.open == nil)
        // the toolbar's monitor first: it folds, then Move sees the same Esc and leaves it alone
        host.toolbar.toggle(.rotate)
        let second = try esc(11)
        host.toolbar.fold(byEscape: second)
        #expect(!feature.escape(second))
        #expect(move.isOn && host.toolbar.open == nil)
        // a later Esc with nothing open: Move goes off
        #expect(feature.escape(try esc(12)))
        #expect(!move.isOn)
    }

    @Test func turningMoveOnCancelsAnArmedPicker() async throws {
        let (host, _, move, _) = try await twoModels()
        let lay = try #require(host.features[RotateFeature.self]?.layFace)
        lay.toggle()
        #expect(lay.armed)
        move.setOn(true)
        #expect(move.isOn)
        #expect(!lay.armed, "one pointer mode at a time")
        lay.toggle()                                  // arming Lay a face flat leaves Move
        #expect(lay.armed)
        #expect(!move.isOn)
    }

    @Test func aModelDroppedOverAnotherPlateGoesToItWithAToastUndoAndMoveStaysOn() async throws {
        let host = ViewerModel()
        try #require(host.controller != nil, "no Metal device")
        await host.load(url: try stl("move-p"))
        let board = try #require(host.features[PlatesFeature.self]?.board)
        let move = try #require(host.features[MoveFeature.self]?.model)
        let tool = try #require(host.controller?.move)
        board.addPlate()
        board.switchPlate(0)
        try await Task.sleep(for: .milliseconds(50))
        #expect(board.plates.count == 2 && board.active == 0)
        move.setOn(true)
        let slot = board.plate.liveSlot
        let from = slot.xy
        // mid-drag over the other plate's bed: it lights; dropped there, the model is on that plate
        #expect(tool.onBegin?(.live, .zero) == true)
        tool.onDrag?(SIMD2(board.pitch, 0))
        #expect(host.controller?.renderer.plateScene.target == 1)
        tool.onEnd?()
        #expect(host.controller?.renderer.plateScene.target == nil)
        #expect(board.active == 1 && board.plate.models.contains { $0 === slot })
        #expect(board.toast?.hasPrefix("Moved ") == true && board.toast?.contains("plate 2") == true, "\(board.toast ?? "")")
        #expect(move.isOn, "the tool stays on through the switch")
        board.undoMove()
        #expect(board.active == 0 && board.plates[0].models.contains { $0 === slot })
        #expect(slot.xy == from)
    }
}

/// The Volume row after a move (`updateFit`, `ui/part.js:472-505`): hangs off the bed, models overlap, fits.
@MainActor
@Suite("Fit after a move")
struct FitAfterMoveTests {
    func stl(_ name: String) throws -> URL {
        URL(fileURLWithPath: try TestParts.write(TestParts.box(-10, 10, -10, 10, 0, 20), name: "\(name)-\(UUID().uuidString).stl"))
    }

    func rows(_ board: PlateBoard, _ host: ViewerModel) -> FitReadout {
        let c = host.controller!
        let f = board.fit
        return FitReadout.make(FitInput(size: (20, 20, 20), volume: (Double(c.volume.x), Double(c.volume.y), Double(c.volume.z)),
                                        spot: (f.spot.x, f.spot.y), parkedOff: f.parkedOff, clash: f.clash))
    }

    @Test func theRowSaysHangsOffTheBedAndFitsAgainAsTheLiveModelIsMoved() async throws {
        let host = ViewerModel()
        try #require(host.controller != nil, "no Metal device")
        await host.load(url: try stl("fit-a"))
        let board = try #require(host.features[PlatesFeature.self]?.board)
        let move = try #require(host.features[MoveFeature.self]?.model)
        let tool = try #require(host.controller?.move)
        #expect(rows(board, host) == FitReadout(text: "fits", warn: false))
        move.setOn(true)
        let bed = Double(host.controller!.volume.x)
        #expect(tool.onBegin?(.live, .zero) == true)
        tool.onDrag?(SIMD2(bed / 2 + 5, 0))                     // past the bed's edge
        tool.onEnd?()
        #expect(rows(board, host) == FitReadout(text: "hangs off the bed", warn: true))
        host.session.undo()
        #expect(rows(board, host) == FitReadout(text: "fits", warn: false))
    }

    @Test func aParkedModelDraggedOffTheBedHangsOffToo() async throws {
        let host = ViewerModel()
        try #require(host.controller != nil, "no Metal device")
        await host.load(url: try stl("fit-b"))
        await host.load(url: try stl("fit-c"), adding: true)
        let board = try #require(host.features[PlatesFeature.self]?.board)
        let move = try #require(host.features[MoveFeature.self]?.model)
        let tool = try #require(host.controller?.move)
        #expect(!board.fit.parkedOff && !board.fit.clash)
        move.setOn(true)
        let parked = board.plate.models[0]
        let bed = Double(host.controller!.volume.x)
        let press = SIMD2(parked.xy.x - board.plate.liveSlot.xy.x, parked.xy.y - board.plate.liveSlot.xy.y)
        #expect(tool.onBegin?(.parked(0), press) == true)
        tool.onDrag?(press + SIMD2(0, bed))                      // far past the bed's back edge
        tool.onEnd?()
        #expect(board.fit.parkedOff)
        #expect(rows(board, host).text == "hangs off the bed")
    }

    @Test func twoModelsSharingACellSayModelsOverlap() async throws {
        let host = ViewerModel()
        try #require(host.controller != nil, "no Metal device")
        await host.load(url: try stl("fit-d"))
        await host.load(url: try stl("fit-e"), adding: true)
        let board = try #require(host.features[PlatesFeature.self]?.board)
        #expect(!board.fit.clash, "Add model leaves them apart")
        // put the live one on the parked one (what Arrange or a bad undo could do)
        board.plate.liveSlot.xy = board.plate.models[0].xy
        #expect(board.fit.clash)
        #expect(rows(board, host) == FitReadout(text: "models overlap", warn: true))
    }
}
