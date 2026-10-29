import Foundation
import SupportFinsCore
import Testing

@testable import Support_Fins

/// The report links of the window (`ui/report.js`): hidden until a build is on screen, then the link and the JSON
/// of that build. The bytes of both are the engine's (`crates/sf-ffi/tests/report.rs` against `web/report.js`).
@MainActor
@Suite("Report")
struct ReportModelTests {
    private func eventually(_ what: String, _ condition: () -> Bool) async throws {
        for _ in 0..<1000 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        Issue.record("timed out: \(what)")
    }

    @Test func theRowWaitsForABuildAndThenReportsIt() async throws {
        let window = ViewerModel()
        try #require(window.controller != nil, "no Metal device")
        let stl = URL(fileURLWithPath: try TestParts.write(
            TestParts.box(-5, 5, -5, 5, 0, 20) + TestParts.box(-30, 30, -30, 30, 20, 24), name: "report-\(UUID().uuidString).stl"))
        await window.load(url: stl)
        let model = try #require(window.features[ReportFeature.self]).model
        #expect(!model.isAvailable, "no build yet: no row")
        #expect(model.current() == nil && model.issueURL() == nil)

        window.session.perform("Add Fins") { window.session.finsOn = true }
        let build = try #require(window.wiring?.build)
        try await eventually("the build") { build.latest != nil && !build.building && model.isAvailable }
        #expect(model.isAvailable)

        let url = try #require(model.issueURL())
        #expect(url.absoluteString.hasPrefix("https://github.com/gittrahan/support-fins/issues/new?template=coverage-report.yml&title=Not+supported%3A+Auto&summary=Site%3A+Support+Fins+for+Mac"))
        let json = try #require(model.current()).settingsJson()
        let rep = try #require(try JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any])
        #expect(rep["v"] as? Int == 1 && rep["threshold"] as? Double == 45 && rep["scale"] as? Double == 1)
        #expect((rep["opts"] as? [String: Any])?["mode"] as? String == "auto")
        #expect(json.contains("\n  \"v\": 1,"), "JSON.stringify(rep, null, 2)")

        // a settings change rebuilds; the row is back once that build lands
        var s = window.session.settings
        s.tineDensity = 60
        window.session.setSettings(s)
        try await eventually("the rebuild") { build.latest != nil && !build.building && model.isAvailable }
        Scratch.remove(stl)
    }

    @Test func todayIsTheUTCDateAndTheSiteNamesTheEngine() {
        #expect(ReportModel.today.range(of: #"^\d{4}-\d{2}-\d{2}$"#, options: .regularExpression) != nil)
        #expect(ReportModel.site.hasPrefix("Support Fins for Mac ") && ReportModel.site.contains("upstream"))
    }
}
