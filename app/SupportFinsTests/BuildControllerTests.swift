import Testing

@testable import Support_Fins

/// A clock the test moves by hand: `sleep` suspends until `advance` reaches its deadline.
@MainActor
final class ManualClock: @unchecked Sendable {
    private struct Sleeper { var id: Int; var deadline: Duration; var resume: CheckedContinuation<Void, any Error> }
    private var now = Duration.zero
    private var sleepers: [Sleeper] = []
    private var nextID = 0
    private var cancelledEarly: Set<Int> = []

    nonisolated func sleep(_ d: Duration) async throws {
        let id = await MainActor.run { () -> Int in nextID += 1; return nextID }
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (k: CheckedContinuation<Void, any Error>) in
                Task { @MainActor in
                    if self.cancelledEarly.remove(id) != nil { k.resume(throwing: CancellationError()); return }
                    self.sleepers.append(Sleeper(id: id, deadline: self.now + d, resume: k))
                }
            }
        } onCancel: {
            Task { @MainActor in
                if let i = self.sleepers.firstIndex(where: { $0.id == id }) {
                    self.sleepers.remove(at: i).resume.resume(throwing: CancellationError())
                } else {
                    self.cancelledEarly.insert(id)
                }
            }
        }
    }

    /// Moves time on and lets whoever was due run.
    func advance(_ d: Duration) async {
        await settleTasks() // let a sleeper that was just started register at the current time
        now += d
        let due = sleepers.filter { $0.deadline <= now }
        sleepers.removeAll { $0.deadline <= now }
        for s in due { s.resume.resume() }
        await settleTasks()
    }

    /// Gives the woken tasks (and the ones they start) turns on the main actor.
    func settleTasks() async {
        for _ in 0..<30 { await Task.yield() }
    }
}

/// Stands in for `BuildModel`: a build stays outstanding until the test lands it.
@MainActor
final class FakeDriver: BuildDriving {
    private(set) var building = false
    private(set) var generation: UInt64 = 0
    private(set) var calls: [String] = []
    private(set) var requests: [(rotation: [Double], options: String)] = []
    private(set) var tunables: [String] = []
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func build(rotation: [Double], optionsJSON: String, tunablesJSON: String) -> UInt64 {
        generation += 1
        building = true
        calls.append("build")
        requests.append((rotation, optionsJSON))
        tunables.append(tunablesJSON)
        return generation
    }
    func cancel() { calls.append("cancel"); finish() }
    func clear() { calls.append("clear") }
    func settled() async {
        guard building else { return }
        await withCheckedContinuation { waiters.append($0) }
    }
    /// The newest build lands.
    func land() { finish() }
    private func finish() {
        building = false
        let w = waiters
        waiters = []
        for c in w { c.resume() }
    }
}

@MainActor
@Suite("BuildController")
struct BuildControllerTests {
    let identity: [Double] = [1, 0, 0, 0, 1, 0, 0, 0, 1]

    func rig(request: (@MainActor () -> BuildRequest?)? = nil) -> (BuildController, FakeDriver, ManualClock, Box) {
        let clock = ManualClock(), driver = FakeDriver(), box = Box()
        let controller = BuildController(driver: driver, request: request ?? { [identity] in
            box.reads += 1
            return BuildRequest(rotation: identity, optionsJSON: box.options)
        }, sleep: { try await clock.sleep($0) })
        return (controller, driver, clock, box)
    }

    final class Box { var options = "{}"; var reads = 0 }

    @Test func aBuildCarriesTheSettingsTunablesAsWellAsTheirOptions() {
        var s = FinSettings()
        s.gap = 0.3
        s.interfaceMaterial = .flat
        let (c, d, _, _) = rig(request: { BuildRequest(rotation: [1, 0, 0, 0, 1, 0, 0, 0, 1], settings: s) })
        c.setEnabled(true)
        #expect(d.requests.first?.options == s.optionsJSON() && d.tunables.first == s.tunablesJSON())
        #expect(d.tunables.first?.contains(#""propGap":0.3"#) == true && d.tunables.first?.contains(#""iface":"flat""#) == true)
    }

    @Test func theDelaysAreTheWebs() {
        #expect(BuildController.debounce == .milliseconds(180))      // ui/settings.js:154
        #expect(BuildController.spinnerDelay == .milliseconds(300))  // ui/finbuild.js:105
    }

    @Test func nothingBuildsWhileTheFinsAreOff() async {
        let (c, d, clock, _) = rig()
        c.refresh()
        c.refreshDebounced()
        await clock.advance(.seconds(1))
        #expect(d.generation == 0 && !c.enabled)
        #expect(d.calls.filter { $0 == "build" }.isEmpty)
    }

    @Test func turningTheFinsOnBuildsAtOnceAndOffDropsAndClears() async {
        let (c, d, clock, _) = rig()
        c.setEnabled(true)
        #expect(d.generation == 1 && d.building)
        c.setEnabled(false)
        #expect(!d.building && d.calls.suffix(2) == ["cancel", "clear"])
        #expect(!c.spinnerVisible)
        c.setEnabled(false) // no change: nothing more
        #expect(d.calls.count == 3)
        _ = clock
    }

    @Test func aBurstOfSettingsChangesBuildsOnceAfter180msOfQuiet() async {
        let (c, d, clock, _) = rig()
        c.setEnabled(true)
        d.land()
        let before = d.generation
        c.refreshDebounced()
        await clock.advance(.milliseconds(100))
        c.refreshDebounced()                      // the clock restarts
        await clock.advance(.milliseconds(100))
        c.refreshDebounced()
        await clock.advance(.milliseconds(179))
        #expect(d.generation == before, "not yet: 179 ms since the last change")
        await clock.advance(.milliseconds(1))
        #expect(d.generation == before + 1)
        await clock.advance(.seconds(2))
        #expect(d.generation == before + 1, "a single build")
    }

    @Test func theRequestIsReadWhenTheRefreshFiresNotWhenAsked() async {
        let (c, d, clock, box) = rig()
        c.setEnabled(true)
        d.land()
        box.options = #"{"mode":"auto"}"#
        c.refreshDebounced()
        box.options = #"{"mode":"full"}"#
        await clock.advance(.milliseconds(180))
        #expect(d.requests.last?.options == #"{"mode":"full"}"#)
    }

    @Test func aNowRefreshCancelsAPendingDebounce() async {
        let (c, d, clock, _) = rig()
        c.setEnabled(true)
        d.land()
        let before = d.generation
        c.refreshDebounced()
        c.refresh()
        #expect(d.generation == before + 1)
        d.land()
        await clock.advance(.seconds(1))
        #expect(d.generation == before + 1, "the debounced one was dropped")
    }

    @Test func theSpinnerShowsOnlyForABuildThatRunsPast300ms() async {
        let (c, d, clock, _) = rig()
        c.setEnabled(true)
        await clock.advance(.milliseconds(299))
        #expect(!c.spinnerVisible)
        await clock.advance(.milliseconds(1))
        #expect(c.spinnerVisible)
        d.land()
        await clock.settleTasks()
        #expect(!c.spinnerVisible, "gone the moment the build lands")
    }

    @Test func aQuickBuildNeverShowsTheSpinner() async {
        let (c, d, clock, _) = rig()
        c.setEnabled(true)
        await clock.advance(.milliseconds(100))
        d.land()
        await clock.settleTasks()
        await clock.advance(.seconds(1))
        #expect(!c.spinnerVisible)
    }

    @Test func aNewerBuildRestartsTheSpinnerClock() async {
        let (c, d, clock, _) = rig()
        c.setEnabled(true)
        await clock.advance(.milliseconds(200))
        c.refresh()                                   // a newer build supersedes it
        await clock.advance(.milliseconds(200))
        #expect(!c.spinnerVisible, "only 200 ms into the newer build")
        await clock.advance(.milliseconds(100))
        #expect(c.spinnerVisible)
        _ = d
    }

    @Test func noPartMeansNoBuild() async {
        let (c, d, _, _) = rig(request: { nil })
        c.setEnabled(true)
        #expect(d.generation == 0 && d.calls.contains("clear"))
    }
}
