import Observation
import SupportFinsCore

/// The Suggest orientation popover's state (`ui/suggest.js`): the ranking runs on the engine's worker when the
/// button is clicked (never on open), the rows come back worded by the engine, and a click on a row turns the
/// part. A newer ranking supersedes an older one (the engine drops it); a manual turn only clears the highlight.
@MainActor
@Observable
final class SuggestModel {
    enum State: Equatable {
        case idle
        case ranking
        case ready(SuggestResult)
        case failed(String)
    }

    private(set) var state = State.idle
    /// The row whose pose the part is in (highlighted), or nil.
    private(set) var picked: Int?
    /// How far the ranking in flight has got, 0...1 (`onProgress` of `suggestOrientations`), or nil before the
    /// first pose is scored. Only rises.
    private(set) var progress: Double?

    @ObservationIgnored private let suggester: Suggester?
    @ObservationIgnored private let pose: PoseAccess
    @ObservationIgnored private var sink: Sink!
    @ObservationIgnored private(set) var wanted: UInt64?

    /// `part` is the part to rank; nil (no part loaded) makes `rank` do nothing.
    init(part: Part?, pose: PoseAccess) {
        suggester = part.map(Suggester.init(part:))
        self.pose = pose
        sink = Sink { [weak self] event in Task { @MainActor in self?.deliver(event) } }
    }

    var rows: [Suggestion] { if case .ready(let r) = state { r.rows } else { [] } }
    var note: String {
        switch state {
        case .ready(let r): r.note
        case .failed(let message): message
        default: ""
        }
    }
    /// The note is the "no support" win, styled good.
    var noteIsGood: Bool { if case .ready(let r) = state { r.noteGood } else { false } }
    var isRanking: Bool { state == .ranking }
    /// The button's label (`Ranking…` while it runs, then `Ranking… 40%` once the engine reports, as the page's
    /// `progress` in `ui/suggest.js`: a long ranking reads as working, not stuck).
    var buttonTitle: String {
        guard isRanking else { return JS.t("Suggest orientation") }
        guard let progress else { return JS.t("Ranking…") }
        // `Math.round(f * 100)`: halves round up
        return "\(JS.t("Ranking…")) \(Int((progress * 100 + 0.5).rounded(.down)))%"
    }

    /// The wide search's candidate density: ten times the web's candidate poses, in less wall time than the web
    /// (docs/PERF-M5.md, "Suggest").
    static let searchDensity = 10.0

    /// Rank the part's poses under the build's options and tunables (`FinSettings.optionsJSON()` / `tunablesJSON()` of
    /// the session's settings: the page's Worker scores with `supportTunables()`, its current gap, pad and nozzle).
    /// `currentBore` is `summary_json`'s `boreWalls` for the pose on screen; `density` the material's g/cm³.
    func rank(thresholdDegrees: Double, optionsJSON: String, tunablesJSON: String, currentBore: Int, density: Double) {
        guard let suggester, !isRanking else { return }
        state = .ranking
        picked = nil
        progress = nil
        wanted = suggester.suggestWide(
            thresholdDeg: thresholdDegrees, optsJson: optionsJSON, tunablesJson: tunablesJSON,
            context: SuggestContext(currentBore: UInt64(max(0, currentBore)), density: density),
            density: Self.searchDensity, listener: sink)
    }

    /// A click on row `index`: turn the part to its pose (one undo step once `PartSession` holds the pose).
    func pick(_ index: Int) {
        guard rows.indices.contains(index) else { return }
        pose.apply(rows[index].rot)
        picked = index
    }

    /// The part's pose changed. A manual turn (the rings, the 90° buttons, lay flat) moves it off the highlighted
    /// row's pose and drops the highlight (`clearSuggestionMark`); the turn a click on that row made does not.
    func poseChanged() {
        guard let i = picked, rows.indices.contains(i) else { picked = nil; return }
        let now = pose.rotation()
        let same = zip(now, rows[i].rot).allSatisfy { abs($0 - $1) < 1e-4 }
        if !same { picked = nil }
    }

    /// The ranking on show and the row picked, kept with a parked model (`suggestState`, `ui/suggest.js`).
    var kept: (result: SuggestResult, picked: Int?)? {
        if case .ready(let r) = state, !r.rows.isEmpty { return (r, picked) }
        return nil
    }

    /// Put a kept ranking back once its model is on show again (`restoreSuggestions`).
    func restore(_ result: SuggestResult, picked: Int?) {
        wanted = nil
        progress = nil
        state = .ready(result)
        self.picked = picked
    }

    /// A new part, an undo or a reset clears the ranking, and one still running is dropped.
    func reset() {
        wanted = nil
        picked = nil
        progress = nil
        state = .idle
    }

    enum Event: Sendable {
        case progress(UInt64, Double)
        case ranked(UInt64, SuggestResult)
        case failed(UInt64, String)
    }

    /// An engine event, on the main actor (the sink hops here; tests call it directly).
    func deliver(_ event: Event) {
        switch event {
        case .progress(let g, let fraction):
            // the engine scores on several threads, so the hops to the main actor can arrive out of order
            guard g == wanted, isRanking else { return }
            progress = max(progress ?? 0, fraction)
        case .ranked(let g, let result):
            guard g == wanted else { return }
            wanted = nil
            progress = nil
            state = .ready(result)
        case .failed(let g, let message):
            guard g == wanted else { return }
            wanted = nil
            progress = nil
            state = .failed(message)
        }
    }

    private final class Sink: SuggestListener, @unchecked Sendable {
        let send: @Sendable (Event) -> Void
        init(_ send: @escaping @Sendable (Event) -> Void) { self.send = send }
        func suggested(generation: UInt64, result: SuggestResult) { send(.ranked(generation, result)) }
        func progress(generation: UInt64, fraction: Double) { send(.progress(generation, fraction)) }
        func strength(generation: UInt64, advice: StrengthAdvice) {}
        func failed(generation: UInt64, err: FfiError) { send(.failed(generation, err.engineMessage)) }
    }
}
