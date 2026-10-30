import AppKit
import Observation
import SupportFinsCore

/// "Something didn't get supported?" and Copy settings (`web/ui/report.js`, local issue 064): the build on screen as a
/// prefilled GitHub issue and as JSON. The numbers and the link are the engine's (`build_report`, `web/report.js`); this
/// is the page's side: what the build on screen was, and when there is one to report.
@MainActor
@Observable
final class ReportModel {
    enum CopyState { case idle, copied, failed }

    @ObservationIgnored weak let host: ViewerModel?
    private(set) var copyState = CopyState.idle
    @ObservationIgnored private var resetTask: Task<Void, Never>?

    init(host: ViewerModel) { self.host = host }

    /// The window closed: the Copied label's reset has nothing left to reset.
    func close() { resetTask?.cancel() }

    /// The build on screen (`builtRequest()`, `ui/finbuild.js`): landed, not outdated by a newer request, for the one
    /// part on show. Nil while a newer one is pending or none is shown.
    private var shown: (build: BuildModel, result: BuildResult, request: BuildRequest)? {
        guard let host, let build = host.wiring?.build, !build.building, let result = build.latest,
              let request = build.latestRequest, request == build.requested, host.session.parts.count == 1
        else { return nil }
        return (build, result, request)
    }

    /// `#report-row` is shown once a build is on screen (`syncReport`).
    var isAvailable: Bool { shown != nil }

    /// The report of the build on screen, or nil when there is none (`current()`).
    func current() -> BuildReport? {
        guard let s = shown else { return nil }
        let scale = host?.features[ScaleFeature.self]?.model.scale ?? 1
        return try? buildReport(result: s.result, scale: scale, optsJson: s.request.optionsJSON, tunablesJson: s.request.tunablesJSON)
    }

    /// The link: this build's settings and numbers, not the model (`report-link`).
    func issueURL() -> URL? {
        guard let host, let report = current() else { return nil }
        let outcomes = host.features[DrawFeature.self]?.model.outcomes ?? [:]
        let drawn = host.session.drawnWalls.filter { outcomes[$0.id]?.ok == true }.count
        // no link when the engine refuses the call (it does not for a date and two counts); `current()` already
        // answers nil the same way when it cannot make the report
        guard let text = try? report.url(site: Self.site, when: Self.today, drawn: UInt32(drawn),
                                         removed: UInt32(host.session.removedFins.count)) else { return nil }
        return URL(string: text)
    }

    func openIssue() {
        if let url = issueURL() { NSWorkspace.shared.open(url) }
    }

    /// `report-copy`: the settings as JSON on the clipboard. The button says "Copied" or "Copy failed" for 1.5 s.
    func copySettings() {
        guard let report = current() else { return }
        let board = NSPasteboard.general
        board.clearContents()
        copyState = board.setString(report.settingsJson(), forType: .string) ? .copied : .failed
        resetTask?.cancel()
        resetTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(1500))
            if !Task.isCancelled { self?.copyState = .idle }
        }
    }

    /// `where: location.host || 'local file'`: the page names its site; the app names itself and the engine it runs.
    static var site: String { "Support Fins for Mac \(engineVersion())" }

    /// `new Date().toISOString().slice(0, 10)`: today's date in UTC.
    static var today: String {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withFullDate]
        return f.string(from: Date())
    }
}
