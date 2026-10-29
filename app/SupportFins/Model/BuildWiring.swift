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
/// - **Dragging a ring**: with "Rebuild While Dragging" on (the default; `liveDrag`), the fins are rebuilt live at the
///   latest pose (`BuildController.refreshLive`) and drawn pale only while they lag the pose; the release does the
///   final build. Off, nothing is built during the drag and the release builds, as the web does. One undo step per
///   drag either way (`RingEdits`).
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
    /// "Rebuild While Dragging" (`ViewSettings.rebuildWhileDragging`), read when a pose arrives.
    @ObservationIgnored private let liveDrag: @MainActor () -> Bool

    init(session: PartSession, controller: ViewerController,
         liveDrag: @escaping @MainActor () -> Bool = { true },
         sleep: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }) {
        self.session = session
        self.controller = controller
        self.liveDrag = liveDrag
        self.sleep = sleep
    }

    /// The progress overlay (shown 300 ms into a build).
    var spinnerVisible: Bool { scheduler?.spinnerVisible ?? false }

    /// A part was loaded (`session.replaceParts` has run): build against its first object from now on.
    /// `reusing`: the part's own build, kept by the plates while the model was parked (`Plates/`); its fins are
    /// shown again at once and rebuilt only if they are not the ones the session asks for.
    func attach(reusing kept: BuildModel? = nil) {
        generation += 1                      // stops the previous file's watch
        scheduler?.setEnabled(false)
        guard let first = session.parts.first else {
            build = nil; scheduler = nil; removal = nil; exporter = nil
            controller.renderer.setSupports(nil)
            return
        }
        let build = kept ?? BuildModel(part: first.part)
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
            source: { [session, controller] in
                session.parts.first.map {
                    .init(partName: $0.info.name, part: $0.part, nozzle: session.settings.nozzle,
                          layerHeight: session.settings.layerHeight, pose: session.pose.columnMajor,
                          threshold: session.settings.threshold,
                          bed: [Double(controller.volume.x), Double(controller.volume.y)],
                          session: session.sessionJSON)
                }
            },
            removed: { [session] in session.removedFins })
        build.onUpdate = { [weak self] in self?.show() }
        scheduler.onLiveBuild = { [weak self] in self?.updateDim() }
        self.build = build
        self.scheduler = scheduler
        self.removal = removal
        self.exporter = exporter
        controller.renderer.setSupports(nil)
        watch()
        // the session's toggle may already be on (a second file in the window)
        scheduler.setEnabled(session.finsOn, reusing: kept != nil)
        if kept != nil { show() }
    }

    /// The model on show is parked (`Plates/`): the build it made stays with it, untouched, and nothing follows
    /// the session for it any more. Returns that build.
    @discardableResult
    func detach() -> BuildModel? {
        generation += 1
        scheduler?.stop()
        let kept = build
        build?.onUpdate = nil
        build = nil; scheduler = nil; removal = nil; exporter = nil
        controller.renderer.setSupports(nil)
        return kept
    }

    /// The window closed: no watch, debounce, live build or spinner goes on, and the build in flight is dropped
    /// (the engine stops it part-way; its delivery finds no one waiting).
    func close() {
        generation += 1
        scheduler?.stop()
        build?.cancel()
        build?.onUpdate = nil
    }

    /// The fins of the latest build on the viewer, minus the removed ones; the removal pointer re-armed to match.
    func show() {
        guard let build, let removal else { return }
        build.show(on: controller.renderer, removed: session.removedFins)
        updateDim()
        removal.update(controller)
    }

    /// The fins are drawn pale while a rebuild is out. During a live drag only while they lag the pose: a build
    /// that is catching up with the ring shows the pale fins of the pose it was made for, and nothing is pale once
    /// the fins on screen are the current pose's.
    func updateDim() {
        guard let build else { controller.renderer.dimSupports = false; return }
        let live = session.ringDragging && liveDrag()
        controller.renderer.dimSupports = build.building && (!live || build.latestRotation != session.pose.columnMajor)
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
        /// A rotate ring is being dragged.
        var dragging: Bool
        /// `PartSession.ringDrags`: a drag that began and ended between two reactions changes it.
        var drags: Int
        var mode: FinSettings.Mode
        var removed: Set<FinKey>
    }

    var key: Key {
        Key(pose: session.pose, options: session.settings.optionsJSON(),
            tunables: session.settings.tunablesJSON(), finsOn: session.finsOn,
            dragging: session.ringDragging, drags: session.ringDrags, mode: session.settings.mode, removed: session.removedFins)
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
        let poseChanged = old?.pose != new.pose || old?.options != new.options || old?.tunables != new.tunables
        if old?.finsOn != new.finsOn { scheduler.setEnabled(new.finsOn) }
        else if new.dragging {
            // a ring is turning the part: live, a build per new pose; off, the fins wait for the release (the web)
            if poseChanged && liveDrag() { scheduler.refreshLive() }
        } else if old?.dragging == true || old?.drags != new.drags { scheduler.endLive() }      // released: the final build
        else if poseChanged { scheduler.refreshDebounced() }
        if old?.mode != new.mode || old?.finsOn != new.finsOn { removal.sync() }
        // The viewer already follows a click (RemovalModel.update); what is left is to make it follow the session
        // when the removed fins changed some other way (undo, redo), or the mode / toggle ended the pointer mode.
        // Compared with what the renderer shows, not with the previous key: a click and its undo can arrive in one
        // turn, and the key then looks unchanged.
        let shown = build?.hiddenRanges(removed: new.removed) ?? []
        if shown != controller.renderer.hiddenFinRanges || old?.mode != new.mode || old?.finsOn != new.finsOn { show() }
        updateDim()
    }
}
