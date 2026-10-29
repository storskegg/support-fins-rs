import SupportFinsCore
import simd

/// The parked models' fins, kept to the plate's settings (`ui/parkedbuild.js`). The settings are the plate's, so a
/// change made on the live model is a change to every model on it: each parked one is rebuilt here, one at a time on
/// the engine's worker (its `BuildModel` is its own), without being made live -- the inspector and the view stay on
/// the model being edited. A parked model's fins stay on screen, greyed, until its new ones land.
///
/// A build is what the live model's would be in its place: its pose, the plate's options and tunables. Its removed
/// fins are masked as the live one's are, not rebuilt.
extension PlateBoard {
    /// The request the plate's settings make for `s`, or nil with the fins off (nothing is built or shown).
    func wantedRequest(for s: ParkedModel) -> BuildRequest? {
        session.finsOn ? BuildRequest(rotation: s.pose.columnMajor, settings: session.settings) : nil
    }

    /// Greyed: built under settings the plate has since changed, or not built yet (`ghostLook`).
    func isStale(_ s: ParkedModel) -> Bool {
        guard let want = wantedRequest(for: s) else { return false }
        return s.build.latestRequest != want
    }

    /// The plate's settings may have changed (a support build was asked for), or a model was parked: grey the parked
    /// fins built under others, and rebuild them once the edits settle (`scheduleParked`, 150 ms).
    func scheduleParked() {
        parkedTimer?.cancel()
        // a build in flight for settings since changed stops (the web ends its Worker)
        if let s = parkedBuilding, s.build.building, s.build.requested != wantedRequest(for: s) { s.build.cancel() }
        parkedTimer = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(150))
            guard !Task.isCancelled else { return }
            self?.pumpParked()
        }
    }

    private func pumpParked() {
        guard parkedTask == nil else { return }
        parkedTask = Task { [weak self] in
            await self?.buildParked()
            self?.parkedTask = nil
        }
    }

    private func buildParked() async {
        while !Task.isCancelled {
            let parked = plate.models.enumerated().compactMap { k, m in k == plate.live ? nil : m.stash }
            guard session.finsOn else {
                // fins off: a parked model keeps none
                for s in parked where s.build.latest != nil || s.build.requested != nil { s.build.clear() }
                sync()
                return
            }
            // a build that failed is not retried until the settings change: `requested` stays what was asked
            guard let s = parked.first(where: { wantedRequest(for: $0) != $0.build.requested }),
                  let want = wantedRequest(for: s) else { sync(); return }
            parkedBuilding = s
            s.build.build(rotation: want.rotation, optionsJSON: want.optionsJSON, tunablesJSON: want.tunablesJSON)
            await s.build.settled()
            parkedBuilding = nil
            sync()
        }
    }

    /// Wait until every parked model of the plate being edited has fins built under its settings (an export writes
    /// them as they are). False when that takes past `seconds` (`parkedSettled`).
    func parkedSettled(seconds: Double = 120) async -> Bool {
        let deadline = ContinuousClock.now + .seconds(seconds)
        while true {
            let parked = plate.models.enumerated().compactMap { k, m in k == plate.live ? nil : m.stash }
            let current = !session.finsOn || parked.allSatisfy { !$0.build.building && $0.build.requested == wantedRequest(for: $0) }
            if current { return true }
            if ContinuousClock.now > deadline { return false }
            if parkedTask == nil { scheduleParked() }
            try? await Task.sleep(for: .milliseconds(50))
        }
    }
}
