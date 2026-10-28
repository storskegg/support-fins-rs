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
    @discardableResult func build(rotation: [Double], optionsJSON: String, tunablesJSON: String) -> UInt64
    func cancel()
    func clear()
    func settled() async
}

extension BuildModel: BuildDriving {}

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
///
/// Time comes from the injected `sleep`, so tests drive it by hand.
@MainActor
@Observable
final class BuildController {
    /// `ui/settings.js:154`
    static let debounce = Duration.milliseconds(180)
    /// `ui/finbuild.js:105`
    static let spinnerDelay = Duration.milliseconds(300)

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
    func setEnabled(_ on: Bool) {
        guard on != enabled else { return }
        enabled = on
        refresh()
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
        guard enabled, let req = request() else {
            // no build wanted now: drop any in flight one so it can't re-add fins (finbuild.js:200-216)
            stopSpinner()
            driver.cancel()
            driver.clear()
            return
        }
        let g = driver.build(rotation: req.rotation, optionsJSON: req.optionsJSON, tunablesJSON: req.tunablesJSON)
        armSpinner(for: g)
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
