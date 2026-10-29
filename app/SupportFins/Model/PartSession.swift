import Foundation
import Observation
import SwiftUI

/// What undo / redo restores (`ui/history.js` `snapshot`): the pose, the settings and the removed fins.
struct SessionSnapshot: Equatable, Sendable {
    var pose: Pose
    var settings: FinSettings
    var removedFins: Set<FinKey>
    /// The "Add fins" toggle (`finsVisible`): the web's history snapshot holds it too (`ui/history.js`), so
    /// turning fins on and off is an undo step.
    var finsOn = false
    /// The hand-placed walls and braces of Draw (`drawnWalls`) and "+ Add walls by hand" (`drawAugment`): the web's
    /// history snapshot holds both (`ui/history.js` `snapshot`).
    var drawnWalls: [DrawnWall] = []
    var addByHand = false
    /// Which model of the plate the step edits (`who` in `ui/history.js`'s snapshot): undoing it edits that model
    /// again. Nil with one model (`Plates/`).
    var owner: UUID?
}

/// The state of one window's part: its objects, the pose, the settings, the fins the user removed, and the
/// undo history over those three. Other tracks code against this: B observes `pose` / `settings`, C binds the
/// inspector to `settings`, the viewer draws `pose`.
///
/// Edits go through `perform` (one step), or through `beginGesture` / `commitGesture` when they arrive as a
/// stream (a ring drag, a slider): the history records one step per gesture. Each step is also registered with
/// the window's `UndoManager`, so Edit > Undo / Redo (⌘Z / ⇧⌘Z) drive the same history.
@MainActor
@Observable
final class PartSession {
    var parts: [CorePart]
    var pose: Pose
    var settings: FinSettings
    var removedFins: Set<FinKey>
    /// Are the fins on (the toolbar's Add fins)?
    var finsOn = false
    /// The hand-placed supports (Features/Draw), in the part's own frame.
    var drawnWalls: [DrawnWall] = []
    /// "+ Add walls by hand" is on (`drawAugment`).
    var addByHand = false
    /// The model on show among a plate's several (`Plates/`): an undo step records it, and an undo or redo brings
    /// that model back first (`focus`) unless it is no longer on the plate (`isOnPlate`), when the step is passed over.
    var owner: UUID?
    @ObservationIgnored var focus: ((UUID) -> Void)?
    @ObservationIgnored var isOnPlate: ((UUID) -> Bool)?
    private(set) var history = History<SessionSnapshot>()
    /// A rotate ring is being dragged (set by `RingEdits`). Not part of the history: the drag is one step, taken
    /// when it ends. The window rebuilds the fins live while this is true (`BuildWiring`).
    var ringDragging = false
    /// How many ring drags have begun: a drag that began and ended between two reactions still shows.
    private(set) var ringDrags = 0
    func ringDragBegan() { ringDragging = true; ringDrags += 1 }
    /// Counts undo / redo restores, so a view can tell "the pose is back" from "the pose never left".
    private(set) var restores = 0

    /// The window's undo manager (SwiftUI's `\.undoManager`); set by the view.
    @ObservationIgnored var undoManager: UndoManager? {
        didSet { undoManager?.levelsOfUndo = History<SessionSnapshot>.limit }
    }

    init(parts: [CorePart] = [], pose: Pose = .identity, settings: FinSettings = FinSettings(),
                removedFins: Set<FinKey> = []) {
        self.parts = parts
        self.pose = pose
        self.settings = settings
        self.removedFins = removedFins
    }

    var snapshot: SessionSnapshot {
        SessionSnapshot(pose: pose, settings: settings, removedFins: removedFins, finsOn: finsOn, drawnWalls: drawnWalls, addByHand: addByHand, owner: owner)
    }
    var canUndo: Bool { history.canUndo }
    var canRedo: Bool { history.canRedo }

    /// A new part: the pose, settings' edits and history start over (`resetPartState`, `resetHistory`).
    func replaceParts(_ parts: [CorePart], keepingSettings: Bool = true) {
        self.parts = parts
        pose = .identity
        removedFins = []
        drawnWalls = []
        addByHand = false
        history.reset()
        if !keepingSettings { settings = FinSettings() }
        undoManager?.removeAllActions(withTarget: self)
    }

    /// Another model of the plate is on show: its own pose and removed fins, under the plate's settings and
    /// history, which go on as they are (`unpark`, `ui/models.js`).
    func install(parts: [CorePart], pose: Pose, removedFins: Set<FinKey>, owner: UUID?) {
        self.parts = parts
        self.pose = pose
        self.removedFins = removedFins
        self.owner = owner
    }

    /// The plate's undo history, taken away when the plate is left (`takeHistory`, `ui/history.js`).
    func takeHistory() -> History<SessionSnapshot> {
        let h = history
        history = History()
        undoManager?.removeAllActions(withTarget: self)
        return h
    }

    /// A plate's history again (`setHistory`): the system's Undo menu follows with one entry per step.
    func setHistory(_ h: History<SessionSnapshot>) {
        history = h
        undoManager?.removeAllActions(withTarget: self)
        for _ in h.undoStack { register(nil) }
    }

    // MARK: edits

    /// One undoable edit: the state before it is recorded when `body` changed something.
    func perform(_ name: String? = nil, _ body: () -> Void) {
        let before = snapshot
        body()
        guard snapshot != before else { return }
        history.push(before)
        register(name)
    }

    /// Replace the pose as one step.
    func setPose(_ p: Pose, name: String? = nil) { perform(name) { pose = p } }

    /// The 90° buttons: a turn about a world axis (`rotate90`, `ui/pose.js:227`).
    func rotate90(about axis: Pose.Axis) { setPose(pose.rotated90(about: axis), name: "Rotate 90° About \(axis.label)") }

    /// Reset Rotation (`ui/pose.js:279`).
    func resetPose() { setPose(.identity, name: "Reset Rotation") }

    /// Open a gesture: the changes made until `commitGesture` are one undo step.
    func beginGesture() { history.beginGesture(from: snapshot) }

    func commitGesture(name: String? = nil) {
        if history.commitGesture(now: snapshot) { register(name) }
    }

    // MARK: settings as a stream of edits

    /// A binding for the inspector: every write goes through `setSettings`.
    var settingsBinding: Binding<FinSettings> { Binding(get: { self.settings }, set: { self.setSettings($0) }) }

    /// The inspector wrote new settings. Between `settingsEditing(true)` and `settingsEditing(false)` (a slider
    /// held down) the writes are one step; a control with no editing phase (a toggle, a menu, a stepper tick, a
    /// text field's commit) makes a step per write. That is the web's boundary: one step per `change` event
    /// (`web/ui/history.js:93-98`), which a slider fires once, on release.
    func setSettings(_ new: FinSettings) {
        guard new != settings else { return }
        // a mode switch starts with hand-placement off (`settings.js` `fin-mode` change: `drawAugment = false`)
        let ends: () -> Void = { if new.mode != self.settings.mode { self.addByHand = false } }
        if history.inGesture { ends(); settings = new } else { perform("Change Settings") { ends(); settings = new } }
    }

    /// A Slider's `onEditingChanged`: true when the thumb goes down, false when it is released.
    func settingsEditing(_ on: Bool) {
        if on { beginGesture() } else { commitGesture(name: "Change Settings") }
    }

    // MARK: undo / redo

    /// Edit > Undo and the toolbar button. Through the `UndoManager` when there is one, so its stacks and ours
    /// stay in step.
    func undo() {
        if let m = undoManager, m.canUndo { m.undo() } else { performUndo() }
    }

    func redo() {
        if let m = undoManager, m.canRedo { m.redo() } else { performRedo() }
    }

    private func performUndo() {
        let accept: (SessionSnapshot) -> Bool = { [isOnPlate] in $0.owner.map { isOnPlate?($0) ?? true } ?? true }
        guard let i = history.undoIndex(accepting: accept) else { return }
        // edit the step's model first, then take the state it leaves for the redo (`step`, `ui/history.js`)
        if let o = history.undoStack[i].owner, o != owner { focus?(o) }
        restore(history.undo(at: i, current: snapshot))
        undoManager?.registerUndo(withTarget: self) { $0.performRedo() }
    }

    private func performRedo() {
        let accept: (SessionSnapshot) -> Bool = { [isOnPlate] in $0.owner.map { isOnPlate?($0) ?? true } ?? true }
        guard let i = history.redoIndex(accepting: accept) else { return }
        if let o = history.redoStack[i].owner, o != owner { focus?(o) }
        restore(history.redo(at: i, current: snapshot))
        undoManager?.registerUndo(withTarget: self) { $0.performUndo() }
    }

    private func restore(_ s: SessionSnapshot) {
        restores += 1
        pose = s.pose
        settings = s.settings
        removedFins = s.removedFins
        finsOn = s.finsOn
        drawnWalls = s.drawnWalls
        addByHand = s.addByHand
    }

    private func register(_ name: String?) {
        guard let m = undoManager else { return }
        m.registerUndo(withTarget: self) { $0.performUndo() }
        if let name, !m.isUndoing, !m.isRedoing { m.setActionName(name) }
    }
}
