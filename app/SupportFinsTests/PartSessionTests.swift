import Foundation
import Testing
@testable import Support_Fins

@MainActor
@Suite("PartSession")
struct PartSessionTests {
    @Test func aRotationIsOneUndoableStep() {
        let s = PartSession()
        s.rotate90(about: .x)
        s.rotate90(about: .z)
        #expect(s.history.undoStack.count == 2 && s.canUndo)
        s.undo()
        #expect(s.pose.isSame(as: Pose.identity.rotated90(about: .x)))
        s.undo()
        #expect(s.pose == .identity && !s.canUndo && s.canRedo)
        s.redo()
        s.redo()
        #expect(s.pose.isSame(as: Pose.identity.rotated90(about: .x).rotated90(about: .z)))
    }

    @Test func aNoOpEditRecordsNothing() {
        let s = PartSession()
        s.resetPose()                                   // already the identity
        s.perform { }
        #expect(s.history.undoStack.isEmpty)
    }

    @Test func aDragIsOneStepHoweverManyEventsItHad() {
        let s = PartSession()
        s.beginGesture()
        for deg in stride(from: 5.0, through: 60, by: 5) { s.pose = Pose.identity.rotated(about: .y, degrees: deg) }
        #expect(s.history.undoStack.isEmpty && !s.canUndo)      // not until the gesture ends
        s.commitGesture(name: "Rotate")
        #expect(s.history.undoStack.count == 1)
        s.undo()
        #expect(s.pose == .identity)
    }

    @Test func editsToTheOtherPartsOfTheSnapshotAreUndoneToo() {
        let s = PartSession()
        let key = FinKey(raw: "wall:0,0,5:10:0")
        s.perform("Remove Fin") { s.removedFins.insert(key) }
        s.rotate90(about: .y)
        s.undo()
        #expect(s.removedFins == [key] && s.pose == .identity)
        s.undo()
        #expect(s.removedFins.isEmpty)
    }

    @Test func theWindowsUndoManagerDrivesTheSameHistory() {
        let s = PartSession()
        let um = UndoManager()
        um.groupsByEvent = false
        s.undoManager = um
        um.beginUndoGrouping()
        s.rotate90(about: .x)
        um.endUndoGrouping()
        #expect(um.canUndo && um.levelsOfUndo == 100)
        #expect(um.undoActionName == "Rotate 90° About X")
        um.undo()                                       // Edit > Undo
        #expect(s.pose == .identity && um.canRedo && s.canRedo)
        um.redo()
        #expect(s.pose.isSame(as: Pose.identity.rotated90(about: .x)))
        // the toolbar button goes through the manager as well, so the two never disagree
        s.undo()
        #expect(s.pose == .identity && !um.canUndo && um.canRedo)
    }

    @Test func aNewPartStartsWithNoHistory() {
        let s = PartSession()
        s.rotate90(about: .z)
        s.replaceParts([])
        #expect(s.pose == .identity && !s.canUndo && !s.canRedo)
    }
}

@MainActor
@Suite("PartSession settings")
struct PartSessionSettingsTests {
    func tweaked(_ s: FinSettings, _ gap: Double) -> FinSettings { var t = s; t.gap = gap; return t }

    @Test func aSliderDragIsOneStepAndASeparateEditAnother() {
        let s = PartSession()
        let t0 = Date(timeIntervalSince1970: 1000)
        let base = s.settings
        for (i, g) in [0.25, 0.3, 0.35].enumerated() { s.setSettings(tweaked(base, g), now: t0.addingTimeInterval(Double(i) * 0.1)) }
        #expect(s.history.undoStack.count == 1)                              // one drag
        s.setSettings(tweaked(base, 0.4), now: t0.addingTimeInterval(5))     // a later edit
        #expect(s.history.undoStack.count == 2)
        s.undo()
        #expect(s.settings.gap == 0.35)
        s.undo()
        #expect(s.settings == base)
    }

    @Test func theInspectorBindingWritesThroughTheSession() {
        let s = PartSession()
        let b = s.settingsBinding
        var v = b.wrappedValue
        v.gap = 0.3
        b.wrappedValue = v
        #expect(s.settings.gap == 0.3 && s.canUndo)
        s.undo()
        #expect(s.settings.gap == FinSettings().gap)
    }
}
