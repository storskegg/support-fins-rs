import Foundation
import Testing

@testable import Support_Fins

/// Crash reports (1D): retention, the banner after a run that did not end normally, the breadcrumb ring and the zip of
/// Save crash report…. The engine's side (the panic hook) is `crates/sf-ffi/tests/crash.rs`.
@MainActor
@Suite("Crash reports")
struct CrashReportsTests {
    private func tempFolder() -> CrashFolder {
        let url = FileManager.default.temporaryDirectory.appending(path: "sf-crash-\(UUID().uuidString)", directoryHint: .isDirectory)
        let folder = CrashFolder(url: url)
        folder.create()
        return folder
    }

    /// A report of `kind` last written `age` seconds ago.
    @discardableResult
    private func plant(_ folder: CrashFolder, _ name: String, age: TimeInterval, now: Date = Date()) throws -> [URL] {
        let files = ["json", "txt"].map { folder.url.appending(path: "crash-\(name).\($0)") }
        for f in files {
            try Data("{}".utf8).write(to: f)
            try FileManager.default.setAttributes([.modificationDate: now.addingTimeInterval(-age)], ofItemAtPath: f.path)
        }
        return files
    }

    @Test func retentionKeepsTheNewest20AndDropsWhatIsOlderThan30Days() throws {
        let folder = tempFolder()
        defer { try? FileManager.default.removeItem(at: folder.url) }
        let now = Date()
        for i in 0..<25 { try plant(folder, "rust-\(i)", age: Double(i) * 60, now: now) }   // 25 fresh ones
        try plant(folder, "old", age: 31 * 24 * 3600, now: now)
        folder.writeMarker()
        try Data("x".utf8).write(to: folder.breadcrumbsURL)

        #expect(folder.reports().count == 26)
        #expect(folder.prune(now: now) == 6, "5 over the cap and the 31-day-old one")
        let kept = folder.reports()
        #expect(kept.count == 20)
        #expect(kept.first?.stem == "crash-rust-0" && kept.last?.stem == "crash-rust-19", "the newest 20 stay: \(kept.map(\.stem))")
        #expect(kept.allSatisfy { $0.files.count == 2 }, "a report goes whole")
        #expect(folder.markerExists && FileManager.default.fileExists(atPath: folder.breadcrumbsURL.path), "markers and breadcrumbs are not reports")

        // and by age alone: 30 days is the line
        let aged = tempFolder()
        defer { try? FileManager.default.removeItem(at: aged.url) }
        try plant(aged, "a", age: 29 * 24 * 3600, now: now)
        try plant(aged, "b", age: 30.5 * 24 * 3600, now: now)
        #expect(aged.prune(now: now) == 1 && aged.reports().map(\.stem) == ["crash-a"])
    }

    @Test func aMarkerAtLaunchShowsTheBannerUntilDismissed() throws {
        let folder = tempFolder()
        defer { try? FileManager.default.removeItem(at: folder.url) }

        // a clean first run: no banner, and a marker is set
        let first = CrashReporter(folder: folder)
        first.start(register: false)
        #expect(!first.showsBanner && folder.markerExists)
        first.note("open file: a.stl (STL), 1 objects, 12 triangles")
        first.note("edit: Add Fins")
        first.stop()                                  // normal termination
        #expect(!folder.markerExists)

        // the next run is clean too
        let second = CrashReporter(folder: folder)
        second.start(register: false)
        #expect(!second.showsBanner)
        second.note("menu: Export Again")
        second.saveNow()
        // ... and this one dies: no stop(), the marker stays

        let third = CrashReporter(folder: folder)
        third.start(register: false)
        #expect(third.showsBanner && third.closedUnexpectedly)
        let report = try #require(folder.reports().first, "a report is written from the saved breadcrumbs")
        #expect(report.stem.hasPrefix("crash-session-") && report.files.count == 2)
        let json = try #require(JSONSerialization.jsonObject(with: Data(contentsOf: report.files[0])) as? [String: Any])
        #expect(json["kind"] as? String == "unexpected-exit")
        #expect((json["breadcrumbs"] as? [String])?.count == 1 && (json["breadcrumbs"] as? [String])?[0].hasSuffix("menu: Export Again") == true)
        #expect(FileManager.default.fileExists(atPath: folder.crashedBreadcrumbsURL.path))

        third.dismiss()
        #expect(!third.showsBanner)
        third.stop()
        let fourth = CrashReporter(folder: folder)
        fourth.start(register: false)
        #expect(!fourth.showsBanner, "dismissed: the report is old news and the marker was cleared")

        // a report that lands later (the engine's panic hook, MetricKit) brings it back
        Thread.sleep(forTimeInterval: 0.02)
        fourth.record(metrics: [.init(kind: "hang", json: Data(#"{"callStackTree":{}}"#.utf8))])
        #expect(fourth.showsBanner || fourth.unseen.count == 1, "a new report since the Dismiss")
        let mk = try #require(folder.reports().first { $0.stem.hasPrefix("crash-metrickit-hang") })
        let mj = try #require(JSONSerialization.jsonObject(with: Data(contentsOf: mk.files[0])) as? [String: Any])
        #expect(mj["diagnostic"] != nil && mj["breadcrumbs"] is [String])
    }

    @Test func theBreadcrumbRingKeepsTheLast50() {
        let b = Breadcrumbs()
        for i in 0..<120 { b.add("action \(i)") }
        let lines = b.lines
        #expect(lines.count == Breadcrumbs.capacity && lines.count == 50)
        #expect(lines.first?.hasSuffix("action 70") == true && lines.last?.hasSuffix("action 119") == true)
    }

    @Test func theZipHoldsTheReportAndTheModelOnlyWhenAsked() throws {
        let folder = tempFolder()
        defer { try? FileManager.default.removeItem(at: folder.url) }
        let files = try plant(folder, "rust-1", age: 0)
        let model = folder.url.appending(path: "Zoë part.stl")
        try Data("solid secret-geometry".utf8).write(to: model)

        let without = try CrashArchive.make(reports: files, model: nil)
        func has(_ d: Data, _ s: String) -> Bool { d.range(of: Data(s.utf8)) != nil }
        #expect(without.prefix(2) == Data("PK".utf8))
        #expect(has(without, "reports/crash-rust-1.json") && has(without, "reports/crash-rust-1.txt"))
        #expect(!has(without, "model/") && !has(without, "secret-geometry"), "unticked: no model")

        let with = try CrashArchive.make(reports: files, model: model)
        #expect(with.range(of: Data("model/Zo".utf8)) != nil && with.range(of: Data(" part.stl".utf8)) != nil, "the model is named, ASCII only")
        #expect(with.range(of: Data("secret-geometry".utf8)) != nil, "ticked: the model is in")
    }
}
