import Foundation
import Observation

/// What a build is asked for: the pose (nine numbers, column-major) and the engine options.
struct BuildRequest: Equatable, Sendable {
    var rotation: [Double]
    var optionsJSON: String
    /// The page's `tunables` (`FinSettings.tunablesJSON()`); `{}` for none.
    var tunablesJSON: String

    init(rotation: [Double], optionsJSON: String, tunablesJSON: String = "{}") {
        (self.rotation, self.optionsJSON, self.tunablesJSON) = (rotation, optionsJSON, tunablesJSON)
    }

    /// The request for a pose and the inspector's settings.
    init(rotation: [Double], settings: FinSettings) {
        self.init(rotation: rotation, optionsJSON: settings.optionsJSON(), tunablesJSON: settings.tunablesJSON())
    }
}

/// The part of `BuildModel` the scheduler drives (a fake stands in for it in tests).
@MainActor
protocol BuildDriving: AnyObject {
    var building: Bool { get }
    var generation: UInt64 { get }
    /// The request the fins on show were built for, when the driver knows it (`BuildModel`); nil otherwise.
    var latestRequest: BuildRequest? { get }
    @discardableResult func build(rotation: [Double], optionsJSON: String, tunablesJSON: String) -> UInt64
    func cancel()
    func clear()
    func settled() async
}

extension BuildModel: BuildDriving {}

extension BuildDriving {
    var latestRequest: BuildRequest? { nil }
}

/// When a build runs, as the web schedules it (`web/ui/finbuild.js refreshFins`, `web/ui/settings.js`):
///
/// - **Add fins** (`fins-toggle`, `finsVisible`): off, nothing is built, an outstanding build is dropped and
///   the fins go (`refreshFins` with the fins off clears the meshes and supersedes the worker). On, the fins
///   are built now.
/// - A settings change asks for a **debounced** refresh (`debouncedRefresh`, `ui/settings.js:154`: 180 ms
///   after the last change); a pose change, the toggle and a part change refresh at once. The request is read
///   when the refresh fires, not when it was asked for, so the newest settings are what get built.
/// - **The spinner** (`armSpinner`, `ui/finbuild.js:101-106`) shows 300 ms after a build starts, only if it is
///   still running, and goes the moment the build lands, fails or is dropped. The fins on screen stay (greyed
///   by the view, `building`) until the fresh build lands.
/// - **Live rebuild while a ring is dragged** (a native improvement; the web rebuilds when the drag ends, and
///   "Rebuild While Dragging" off keeps that): `refreshLive()` per new pose builds at once, with no debounce, and
///   a newer pose supersedes a build in flight that is younger than `liveLandAfter` (the engine stops the stale
///   one part-way). An older build is let land instead, so a build slower than the pose events still shows
///   (the fins are drawn pale while they lag the pose); the newest pose is then built next. `endLive()` on the
///   release does the final build at the released pose.
///
/// Time comes from the injected `sleep`, so tests drive it by hand.
@MainActor
@Observable
final class BuildController {
    /// `ui/settings.js:154`
    static let debounce = Duration.milliseconds(180)
    /// `ui/finbuild.js:105`
    static let spinnerDelay = Duration.milliseconds(300)
    /// How long a live build may run before a newer pose stops superseding it and waits for it to land. A build
    /// younger than this has done little, and its pose is already stale, so the newer pose replaces it; an older
    /// one is let finish, or a build slower than the pose events (60 Hz, 17 ms; a 22 ms torus) would be superseded
    /// every time and the fins would not move until the ring paused. Measured with `bench_drag` on 8, 25 and 100 ms
    /// and never / always: at 8 ms the fins trail the ring by 1-5 degrees on the stress proxies; always (or 25 /
    /// 100 ms, which are the same for builds under that) lands 1 build in 90 and trails by 15-23 degrees.
    static let liveLandAfter = Duration.milliseconds(8)

    /// The "Add fins" toggle (`fins-toggle`).
    private(set) var enabled = false
    /// The progress overlay.
    private(set) var spinnerVisible = false

    @ObservationIgnored let driver: any BuildDriving
    @ObservationIgnored private let request: @MainActor () -> BuildRequest?
    @ObservationIgnored private let sleep: @Sendable (Duration) async throws -> Void
    @ObservationIgnored private var pending: Task<Void, Never>?
    @ObservationIgnored private var spinner: Task<Void, Never>?
    @ObservationIgnored private var watcher: Task<Void, Never>?
    /// Called after a live build starts without the window having asked (a newer pose waited for the last one to
    /// land), so the window can restyle the fins.
    @ObservationIgnored var onLiveBuild: (@MainActor () -> Void)?
    @ObservationIgnored private var liveActive = false
    @ObservationIgnored private var liveMayLand = false
    @ObservationIgnored private var livePending = false
    @ObservationIgnored private var liveTimer: Task<Void, Never>?
    @ObservationIgnored private var liveChain: Task<Void, Never>?
    @ObservationIgnored private var lastRequest: BuildRequest?

    /// - Parameters:
    ///   - request: the pose and options to build *now*, or nil when there is no part to build.
    ///   - sleep: waits for a duration (`Task.sleep` by default).
    init(driver: any BuildDriving,
         request: @escaping @MainActor () -> BuildRequest?,
         sleep: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }) {
        self.driver = driver
        self.request = request
        self.sleep = sleep
    }

    /// Turns the fins on or off. On builds at once; off drops any build in flight and clears the fins.
    /// `reusing`: a model brought back to the plate (`Plates/`) keeps the fins it was built with when they are
    /// the ones the request would build, so there is no rebuild (and no blink) on a switch.
    func setEnabled(_ on: Bool, reusing: Bool = false) {
        guard on != enabled else { return }
        enabled = on
        if reusing, on, !driver.building, let req = request(), driver.latestRequest == req {
            lastRequest = req
            return
        }
        refresh()
    }

    /// Stops following the session without touching the driver or the fins it holds: the model this scheduler
    /// served was parked (`Plates/`), and its build stays as it stands. No debounced refresh, live build or
    /// spinner outlives the call.
    func stop() {
        pending?.cancel(); pending = nil
        stopLive()
        stopSpinner()
        enabled = false
    }

    /// A settings change: refresh 180 ms after the last one (`debouncedRefresh`).
    func refreshDebounced() {
        pending?.cancel()
        let sleep = self.sleep
        pending = Task { [weak self] in
            do { try await sleep(Self.debounce) } catch { return }
            guard !Task.isCancelled, let self else { return }
            self.pending = nil
            self.refresh()
        }
    }

    /// Refresh now (a pose change, the toggle, a new part): `refreshFins`.
    func refresh() {
        pending?.cancel()
        pending = nil
        stopLive()
        guard enabled, let req = request() else {
            // no build wanted now: drop any in flight one so it can't re-add fins (finbuild.js:200-216)
            stopSpinner()
            lastRequest = nil
            driver.cancel()
            driver.clear()
            return
        }
        lastRequest = req
        let g = driver.build(rotation: req.rotation, optionsJSON: req.optionsJSON, tunablesJSON: req.tunablesJSON)
        armSpinner(for: g)
    }

    // MARK: live drag

    /// A new pose while a ring is dragged: build it now. A build in flight is superseded (the engine stops it)
    /// unless it has already run `liveLandAfter`; that one is allowed to land, and the newest pose is built then.
    func refreshLive() {
        pending?.cancel()
        pending = nil
        guard enabled else { return }
        liveActive = true
        if driver.building, liveMayLand {
            livePending = true
            return
        }
        guard let req = request() else { return }
        livePending = false
        liveMayLand = false
        lastRequest = req
        let g = driver.build(rotation: req.rotation, optionsJSON: req.optionsJSON, tunablesJSON: req.tunablesJSON)
        armSpinner(for: g)
        let sleep = self.sleep
        liveTimer?.cancel()
        liveTimer = Task { [weak self] in
            do { try await sleep(Self.liveLandAfter) } catch { return }
            guard !Task.isCancelled, let self, self.driver.generation == g else { return }
            self.liveMayLand = true
        }
        liveChain?.cancel()
        liveChain = Task { [weak self] in
            guard let self else { return }
            await self.driver.settled()
            guard !Task.isCancelled, self.driver.generation == g, self.livePending, self.liveActive else { return }
            self.refreshLive()
        }
        onLiveBuild?()
    }

    /// The ring was released: the final build at the released pose, at once. The live build in flight is that
    /// build when it was made for the same request; otherwise the pose moved on after it, and it is superseded.
    func endLive() {
        let wasLive = liveActive
        stopLive()
        guard enabled, let req = request() else { refresh(); return }
        if wasLive, driver.building, lastRequest == req { return }
        refresh()
    }

    private func stopLive() {
        liveActive = false
        livePending = false
        liveMayLand = false
        liveTimer?.cancel(); liveTimer = nil
        liveChain?.cancel(); liveChain = nil
    }

    // MARK: spinner

    private func armSpinner(for g: UInt64) {
        spinner?.cancel()
        watcher?.cancel()
        spinnerVisible = false
        let sleep = self.sleep
        spinner = Task { [weak self] in
            do { try await sleep(Self.spinnerDelay) } catch { return }
            guard !Task.isCancelled, let self, self.driver.building, self.driver.generation == g else { return }
            self.spinnerVisible = true
        }
        // gone the moment the newest build lands, fails or is dropped
        watcher = Task { [weak self] in
            guard let self else { return }
            await self.driver.settled()
            guard !Task.isCancelled, self.driver.generation == g else { return }
            self.stopSpinner()
        }
    }

    private func stopSpinner() {
        spinner?.cancel()
        spinner = nil
        watcher?.cancel()
        watcher = nil
        spinnerVisible = false
    }
}
