import Foundation
import Testing
@testable import Support_Fins

@Suite("History")
struct HistoryTests {
    @Test func undoThenRedoWalkTheSnapshotsInOrder() {
        var h = History<Int>()
        var state = 0
        for next in 1...3 { h.push(state); state = next }              // 0 -> 1 -> 2 -> 3
        #expect(h.undoStack == [0, 1, 2] && h.canUndo && !h.canRedo)
        state = h.undo(current: state)!
        #expect(state == 2)
        state = h.undo(current: state)!
        #expect(state == 1 && h.redoStack == [3, 2])
        state = h.redo(current: state)!
        #expect(state == 2)
        state = h.redo(current: state)!
        #expect(state == 3 && !h.canRedo)
        let none = h.redo(current: state)
        #expect(none == nil)
    }

    @Test func aFreshActionDropsTheRedoStack() {
        var h = History<Int>()
        h.push(0)
        _ = h.undo(current: 1)
        #expect(h.canRedo)
        h.push(0)
        #expect(!h.canRedo)
    }

    @Test func keepsTheLast100Steps() {
        var h = History<Int>()
        for i in 0..<150 { h.push(i) }
        #expect(h.undoStack.count == 100 && h.undoStack.first == 50 && h.undoStack.last == 149)
    }

    @Test func aGestureIsOneStepAndOnlyIfItChangedSomething() {
        var h = History<Int>()
        h.beginGesture(from: 10)
        h.beginGesture(from: 99)                       // a second begin while open is ignored
        #expect(h.inGesture && !h.canUndo)
        let recorded = h.commitGesture(now: 40)         // many events, one step, from where it began
        #expect(recorded)
        #expect(h.undoStack == [10] && !h.inGesture)
        h.beginGesture(from: 40)
        let nothing = h.commitGesture(now: 40)          // nothing changed: nothing recorded
        #expect(!nothing)
        #expect(h.undoStack == [10])
        let closed = h.commitGesture(now: 41)           // no open gesture
        #expect(!closed)
    }

    @Test func undoAndRedoWaitForAnOpenGesture() {
        var h = History<Int>()
        h.push(1)
        h.beginGesture(from: 2)
        let blocked = h.undo(current: 2)
        #expect(blocked == nil && !h.canUndo)
        h.commitGesture(now: 3)
        let back = h.undo(current: 3)
        #expect(back == 2)
    }

    @Test func resetForgetsEverything() {
        var h = History<Int>()
        h.push(1)
        h.beginGesture(from: 2)
        h.reset()
        #expect(!h.canUndo && !h.canRedo && !h.inGesture)
    }
}
