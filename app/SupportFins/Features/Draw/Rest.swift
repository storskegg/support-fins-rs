import Foundation

/// "Run this once the pointer has rested" (`onRest`, `web/ui/rest.js`). A hover preview that costs tens of ms (the Sway
/// brace ghost builds a whole brace per new spot) would stall every frame of a sweep across a face if it ran per move.
/// `poke(_:)` records the latest target and restarts a short timer; `fn` runs once with that target when no poke has
/// come for `ms`. `cancel()` drops a pending run (a click, the pointer leaving the part).
@MainActor
final class Rest<Target> {
    /// `REST_MS`.
    static var restMS: Int { 80 }

    private let ms: Int
    private let fn: (Target) -> Void
    private var task: Task<Void, Never>?
    private var latest: Target?

    init(ms: Int = Rest.restMS, _ fn: @escaping (Target) -> Void) {
        self.ms = ms
        self.fn = fn
    }

    /// A run is waiting for the pointer to rest.
    var pending: Bool { task != nil }

    func poke(_ target: Target) {
        latest = target
        task?.cancel()
        task = Task { [weak self, ms] in
            try? await Task.sleep(for: .milliseconds(ms))
            guard !Task.isCancelled, let self else { return }
            task = nil
            let t = latest
            latest = nil
            if let t { fn(t) }
        }
    }

    func cancel() {
        task?.cancel()
        task = nil
        latest = nil
    }
}
