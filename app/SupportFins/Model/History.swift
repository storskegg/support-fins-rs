import Foundation

/// Undo / redo as a whole-state snapshot stack, not a command log (`web/ui/history.js`): every mutation records
/// the state BEFORE it, a fresh action empties the redo stack, and undo / redo swap the current state between
/// the two stacks. 100 steps are kept (`history.js:histPush`).
///
/// A gesture that changes state over many events (a rotate-ring drag) is one step: take the snapshot when it
/// starts, record it only if the gesture changed something, and undo / redo wait until it ends
/// (`beginGesture` / `commitGesture`, `history.js:111`).
struct History<State: Equatable & Sendable>: Sendable {
    static var limit: Int { 100 }

    private(set) var undoStack: [State] = []
    private(set) var redoStack: [State] = []
    /// The state a gesture started from, while one is open.
    private var gestureStart: State?

    init() {}

    var canUndo: Bool { !undoStack.isEmpty && !inGesture }
    var canRedo: Bool { !redoStack.isEmpty && !inGesture }
    var inGesture: Bool { gestureStart != nil }

    /// Record `before` (the state prior to a mutation) as an undo step; the redo stack is dropped.
    mutating func push(_ before: State) {
        undoStack.append(before)
        if undoStack.count > Self.limit { undoStack.removeFirst() }
        redoStack.removeAll()
    }

    /// Open a gesture at the state it starts from. A second begin while one is open is ignored.
    mutating func beginGesture(from state: State) {
        if gestureStart == nil { gestureStart = state }
    }

    /// Close the gesture; it becomes one step when `now` differs from where it started.
    @discardableResult
    mutating func commitGesture(now: State) -> Bool {
        guard let start = gestureStart else { return false }
        gestureStart = nil
        guard start != now else { return false }
        push(start)
        return true
    }

    /// Step back: returns the state to restore, with `current` going onto the redo stack. Nil if there is none
    /// (or a gesture is open).
    mutating func undo(current: State) -> State? {
        guard canUndo, let previous = undoStack.popLast() else { return nil }
        redoStack.append(current)
        return previous
    }

    mutating func redo(current: State) -> State? {
        guard canRedo, let next = redoStack.popLast() else { return nil }
        undoStack.append(current)
        return next
    }

    /// A new part: undo history does not carry across parts (`resetHistory`).
    mutating func reset() {
        undoStack.removeAll()
        redoStack.removeAll()
        gestureStart = nil
    }
}
