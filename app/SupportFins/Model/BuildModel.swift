import Foundation
import Observation
import SupportFinsCore

/// The fins of one part: runs the engine's `Builder` for a pose and settings, and publishes what came back.
///
/// One `BuildModel` per part (docs/FFI.md, M4b). A request goes to the `Builder`, which runs it on a Rust
/// worker thread and drops a job that is no longer the newest (generations); this model hops the delivery to
/// the main actor, decodes the JSON the engine returns, and ignores anything that is not the newest request it
/// made (so a `cancel()` or a newer `build` can never be overtaken by an older result).
///
/// The web builds the same way (`ui/finbuild.js` `refreshFins`): the fins on screen stay until the fresh build
/// lands, so `latest` is kept while `building` is true.
@MainActor
@Observable
final class BuildModel {
    /// The newest successful build, or nil before one lands (or after `clear()`).
    private(set) var latest: BuildResult?
    /// `latest.summaryJson()` decoded: the readout's numbers.
    private(set) var summary: BuildSummary?
    /// `latest.recordsJson()` decoded: the per-fin and per-wall records.
    private(set) var records: BuildRecords?
    /// Each of `records.fins`' key (`FinKey.assign`), in the same order.
    private(set) var keys: [FinKey.Assigned] = []
    /// The pose (nine numbers, column-major) `latest` was built in: the fins lag the pose while it differs from the
    /// session's (a live rebuild while a ring is dragged).
    private(set) var latestRotation: [Double]?
    /// The request `latest` was built for (pose, options, tunables): equal to the one a refresh would make, the
    /// fins are current. Nil before a build lands and after `clear()`.
    private(set) var latestRequest: BuildRequest?
    /// The request of the newest `build()` call still wanted (in flight or landed); nil after `cancel()` / `clear()`.
    /// A parked model's build is stale when this is not the request its plate's settings would make (`Plates/`).
    private(set) var requested: BuildRequest?
    /// A build is outstanding.
    private(set) var building = false
    /// The newest request's failure (the engine's JS-identical message), or nil.
    private(set) var error: String?
    /// The generation of the newest request, 0 before any.
    private(set) var generation: UInt64 = 0

    @ObservationIgnored let part: Part
    /// Called when `latest` changes: a build landed, or `clear()` forgot it. The window puts the new fins on the
    /// viewer here.
    @ObservationIgnored var onUpdate: (@MainActor () -> Void)?
    @ObservationIgnored let builder: Builder
    @ObservationIgnored private var listener: Listener!
    /// The generation whose delivery is still wanted; nil after `cancel()` / `clear()`.
    @ObservationIgnored private var wanted: UInt64?
    @ObservationIgnored private var wantedRotation: [Double]?
    @ObservationIgnored private var waiters: [CheckedContinuation<Void, Never>] = []

    init(part: Part) {
        self.part = part
        builder = Builder(part: part)
        listener = Listener { [weak self] event in
            Task { @MainActor in self?.apply(event) }
        }
    }

    /// Starts a build of the part in the pose `rotation` (nine numbers, column-major) with `optionsJSON`
    /// (`options.json` keys, `{}` for the engine defaults) and the page's `tunablesJSON` (`{}` for none) and returns its generation. Supersedes any build in
    /// flight: the engine drops the older job, and its result is ignored here.
    @discardableResult
    func build(rotation: [Double], optionsJSON: String, tunablesJSON: String = "{}") -> UInt64 {
        error = nil
        building = true
        CrashReporter.setSettings(options: optionsJSON, tunables: tunablesJSON)
        CrashReporter.note("build requested")
        let g = builder.build(rot: rotation, optsJson: optionsJSON, tunablesJson: tunablesJSON, listener: listener)
        generation = g
        wanted = g
        wantedRotation = rotation
        requested = BuildRequest(rotation: rotation, optionsJSON: optionsJSON, tunablesJSON: tunablesJSON)
        return g
    }

    /// Abandons the build in flight (the fins toggle went off, the part changed): the engine stops it part-way
    /// (`Builder.cancel`), and its result will not be delivered here. The fins already shown stay until `clear()`.
    func cancel() {
        builder.cancel()
        requested = latestRequest
        wanted = nil
        building = false
        resumeWaiters()
    }

    /// Forgets the build: no fins, no records (`refreshFins` with the fins off, `ui/finbuild.js:200-216`).
    func clear() {
        cancel()
        error = nil
        latest = nil
        latestRotation = nil
        latestRequest = nil
        requested = nil
        summary = nil
        records = nil
        keys = []
        onUpdate?()
    }

    /// Returns when no build is outstanding.
    func settled() async {
        guard building else { return }
        await withCheckedContinuation { waiters.append($0) }
    }

    /// The records of `removed` fins that this build matches (`matchRemoved`): record id -> the key it matched.
    /// A key with no fin near it (another pose) matches nothing, and stays in the caller's set for later.
    func matchedRemovals(of removed: Set<FinKey>) -> [Int: FinKey] {
        guard let records, !removed.isEmpty else { return [:] }
        return FinKey.matchRemoved(records: records.fins, keys: keys, removed: removed)
    }

    // MARK: Delivery

    fileprivate enum Event: Sendable {
        case done(UInt64, BuildResult)
        case failed(UInt64, FfiError)
    }

    private func apply(_ event: Event) {
        switch event {
        case .done(let g, let result):
            guard g == wanted else { return }
            do {
                let summary = try BuildSummary(json: result.summaryJson())
                let records = try BuildRecords(json: result.recordsJson())
                self.summary = summary
                self.records = records
                keys = FinKey.assign(records.fins)
                latest = result
                latestRotation = wantedRotation
                latestRequest = requested
                error = nil
            } catch {
                // the engine's JSON is the contract (docs/FFI.md): a mismatch is a bug to see, not to hide
                self.error = "The build's records could not be read: \(error)"
            }
        case .failed(let g, let err):
            guard g == wanted else { return }
            error = Self.message(err)
        }
        building = false
        wanted = nil
        resumeWaiters()
        if case .done = event { onUpdate?() }
    }

    private func resumeWaiters() {
        let w = waiters
        waiters = []
        for c in w { c.resume() }
    }

    /// The text of an engine error (JS-identical where JS has one).
    static func message(_ err: FfiError) -> String {
        switch err {
        case .Io(let message), .Format(let message), .Engine(let message): return message
        case .LimitExceeded(_, _, _, let message): return message
        case .Cancelled: return JS.t("The build was cancelled.")
        }
    }
}

/// The `Builder`'s callbacks arrive on its worker thread; this forwards them as `Sendable` events.
private final class Listener: BuildListener, @unchecked Sendable {
    private let send: @Sendable (BuildModel.Event) -> Void
    init(_ send: @escaping @Sendable (BuildModel.Event) -> Void) { self.send = send }
    func done(generation: UInt64, result: BuildResult) { send(.done(generation, result)) }
    func failed(generation: UInt64, err: FfiError) { send(.failed(generation, err)) }
}
