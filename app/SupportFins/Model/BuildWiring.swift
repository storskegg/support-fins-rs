import Foundation
import Observation
import Viewer

/// The fins of one window: connects the session (pose, settings, removed fins, Add fins) to the build, and the
/// build to the viewer, the export menu and Remove fins mode.
///
/// - **Builds** (`BuildController`): a pose or settings change refreshes 180 ms after the last one
///   (`debouncedRefresh`), the Add fins toggle at once. Only settings the engine takes cause a rebuild: the
///   request is the pose, `FinSettings.optionsJSON()` and `tunablesJSON()`, so a display-only setting (Highlight small
///   overhangs) changes nothing there.
/// - **Landing**: the fins and pad go on the viewer (no copy), minus the removed fins; the fins on screen
///   stay, paler, while a rebuild is out.
/// - **Removal**: the removed keys are the session's `removedFins`, so a click on a fin and Restore all are undo
///   steps. A removal only masks vertex ranges; it never rebuilds.
/// - **Export**: the part, the build and the nozzle / layer height the file name carries.
///
/// One `BuildModel` per part (the engine's `Builder` is per part); until plates arrive (M6) the first object of a
/// file is the one that is built and exported.
@MainActor
@Observable
final class BuildWiring {
    private(set) var build: BuildModel?
    private(set) var scheduler: BuildController?
    private(set) var removal: RemovalModel?
    private(set) var exporter: ExportModel?

    @ObservationIgnored let session: PartSession
    @ObservationIgnored let controller: ViewerController
    @ObservationIgnored private let sleep: @Sendable (Duration) async throws -> Void

    init(session: PartSession, controller: ViewerController,
         sleep: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }) {
        self.session = session
        self.controller = controller
        self.sleep = sleep
    }

    /// The progress overlay (shown 300 ms into a build).
    var spinnerVisible: Bool { scheduler?.spinnerVisible ?? false }

    /// A part was loaded (`session.replaceParts` has run): build against its first object from now on.
    func attach() {
        generation += 1                      // stops the previous file's watch
        scheduler?.setEnabled(false)
        guard let first = session.parts.first else {
            build = nil; scheduler = nil; removal = nil; exporter = nil
            controller.renderer.setSupports(nil)
            return
        }
        let build = BuildModel(part: first.part)
        let scheduler = BuildController(
            driver: build,
            request: { [session] in BuildRequest(rotation: session.pose.columnMajor, settings: session.settings) },
            sleep: sleep)
        let removal = RemovalModel(
            build: build,
            finsOn: { [session] in session.finsOn },
            mode: { [session] in session.settings.mode },
            removed: (read: { [session] in session.removedFins },
                      write: { [session] new, name in session.perform(name) { session.removedFins = new } }))
        let exporter = ExportModel(
            build: build, finsOn: { [session] in session.finsOn },
            source: { [session] in
                session.parts.first.map {
                    .init(partName: $0.info.name, part: $0.part, nozzle: session.settings.nozzle,
                          layerHeight: session.settings.layerHeight)
                }
            },
            objectCount: { [session] in session.parts.count })
        build.onUpdate = { [weak self] in self?.show() }
        self.build = build
        self.scheduler = scheduler
        self.removal = removal
        self.exporter = exporter
        controller.renderer.setSupports(nil)
        watch()
        // the session's toggle may already be on (a second file in the window)
        scheduler.setEnabled(session.finsOn)
    }

    /// The fins of the latest build on the viewer, minus the removed ones; the removal pointer re-armed to match.
    func show() {
        guard let build, let removal else { return }
        build.show(on: controller.renderer, removed: session.removedFins)
        controller.renderer.dimSupports = build.building
        removal.update(controller)
    }

    /// Esc (the window's exit command, wherever the focus is; the viewer also takes it when it has the keys):
    /// ends Remove fins mode. True when it did.
    @discardableResult
    func escape() -> Bool {
        guard let removal, removal.armed else { return false }
        removal.disarm()
        show()
        return true
    }

    // MARK: following the session

    /// What a change of the session can mean for the fins.
    struct Key: Equatable {
        var pose: Pose
        var options: String
        /// The page's tunables (`FinSettings.tunablesJSON()`): a gap or a wall-lines change rebuilds too.
        var tunables: String
        var finsOn: Bool
        var mode: FinSettings.Mode
        var removed: Set<FinKey>
    }

    var key: Key {
        Key(pose: session.pose, options: session.settings.optionsJSON(),
            tunables: session.settings.tunablesJSON(), finsOn: session.finsOn,
            mode: session.settings.mode, removed: session.removedFins)
    }

    /// Re-arming `withObservationTracking`: `onChange` fires once, just before the first change, so the reaction runs
    /// on the next main-actor turn (the state is final by then, and a burst of changes is one reaction) and arms it
    /// again.
    private func watch() {
        generation += 1
        last = key
        arm(generation)
    }

    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var last: Key?

    private func arm(_ g: Int) {
        withObservationTracking {
            _ = self.key
        } onChange: { [weak self] in
            Task { @MainActor [weak self] in
                guard let self, self.generation == g else { return }
                let now = self.key
                self.react(from: self.last, to: now)
                self.last = now
                self.arm(g)
            }
        }
    }

    /// One change of the session -> the right amount of work.
    func react(from old: Key?, to new: Key) {
        guard let scheduler, let removal else { return }
        if old?.finsOn != new.finsOn { scheduler.setEnabled(new.finsOn) }
        else if old?.pose != new.pose || old?.options != new.options || old?.tunables != new.tunables { scheduler.refreshDebounced() }
        if old?.mode != new.mode || old?.finsOn != new.finsOn { removal.sync() }
        // The viewer already follows a click (RemovalModel.update); what is left is to make it follow the session
        // when the removed fins changed some other way (undo, redo), or the mode / toggle ended the pointer mode.
        // Compared with what the renderer shows, not with the previous key: a click and its undo can arrive in one
        // turn, and the key then looks unchanged.
        let shown = build?.hiddenRanges(removed: new.removed) ?? []
        if shown != controller.renderer.hiddenFinRanges || old?.mode != new.mode || old?.finsOn != new.finsOn { show() }
        controller.renderer.dimSupports = build?.building ?? false
    }
}
