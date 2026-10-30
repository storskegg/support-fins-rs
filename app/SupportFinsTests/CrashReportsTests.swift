import AppKit
import Foundation
import Observation
import SwiftUI
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

    @Test func aCaughtPanicReportAloneNeverRaisesTheBanner() throws {
        let folder = tempFolder()
        defer { try? FileManager.default.removeItem(at: folder.url) }
        try plant(folder, "rust-2026-10-29T10-00-00-0", age: 5)       // the panic hook's report; the app went on
        let clean = CrashReporter(folder: folder)
        clean.start(register: false)
        #expect(!clean.showsBanner && clean.unseen.count == 1, "the report is there, the banner is not")
        #expect(folder.reports().count == 1, "it stays for Help > Show crash reports")
        clean.refresh()
        #expect(!clean.showsBanner)
        clean.stop()

        // a MetricKit crash or hang diagnostic does
        let next = CrashReporter(folder: folder)
        next.start(register: false)
        next.record(metrics: [.init(kind: "crash", json: Data("{}".utf8))])
        #expect(next.showsBanner)
    }

    #if DEBUG
    /// The launch arguments `-SFCrashDir <name> -SFPlantCrash YES` (UserDefaults' argument domain): the marker is planted in
    /// that folder (in the app's own temporary directory) before `start` looks for a leftover one, so the banner shows;
    /// and a test host that carries the XCTest environment still starts when it names its own folder.
    @Test func plantingTheMarkerBeforeStartShowsTheBanner() throws {
        let folder = tempFolder()
        defer { try? FileManager.default.removeItem(at: folder.url) }
        let d = UserDefaults.standard
        d.set(folder.url.path, forKey: "SFCrashDir")
        d.set(true, forKey: "SFPlantCrash")
        defer { d.removeObject(forKey: "SFCrashDir"); d.removeObject(forKey: "SFPlantCrash") }
        let env = ["XCTestConfigurationFilePath": "/x"]
        #expect(CrashReporter.shouldStart(environment: env), "a named crash folder starts the reporter in a test environment")
        let r = CrashReporter(folder: CrashFolder(url: FileManager.default.temporaryDirectory.appending(path: "unused-\(UUID().uuidString)")))
        r.start(register: false)
        #expect(r.showsBanner && r.closedUnexpectedly)
        defer { try? FileManager.default.removeItem(at: r.folder.url) }
        #expect(r.folder.url.standardizedFileURL == CrashFolder.testFolder(named: folder.url.path).standardizedFileURL)
        #expect(r.folder.reports().contains { $0.stem.hasPrefix("crash-session-") })
        #expect(r.folder.markerExists, "this run's own marker is set after the leftover one was read")
    }

    /// The UI tests' `-SFCrashDir` names a folder in the test runner's temporary directory, which is the runner's
    /// sandbox container (`~/Library/Containers/<runner>/Data/tmp/sf-uitest-<UUID>/crash-reports`). The app is sandboxed
    /// too and cannot write there: the marker was never planted and the banner never showed (PR #213's UI test). The
    /// folder the app uses must be one it can write, so the planted marker is there at the leftover check.
    @Test func aCrashFolderNamedByTheSandboxedUITestRunnerIsOneTheAppCanWrite() throws {
        guard let pw = getpwuid(getuid()) else { Issue.record("no home"); return }
        let home = String(cString: pw.pointee.pw_dir)       // the real home: NSHomeDirectory() is the app's container
        let id = "sf-uitest-\(UUID().uuidString)"
        let runnerPath = "\(home)/Library/Containers/com.storskegg.SupportFinsUITests.xctrunner/Data/tmp/\(id)/crash-reports"
        let d = UserDefaults.standard
        d.set(runnerPath, forKey: "SFCrashDir")
        d.set(true, forKey: "SFPlantCrash")
        defer { d.removeObject(forKey: "SFCrashDir"); d.removeObject(forKey: "SFPlantCrash") }
        let r = CrashReporter(folder: CrashFolder(url: FileManager.default.temporaryDirectory.appending(path: "unused-\(UUID().uuidString)")))
        r.start(register: false)
        defer { try? FileManager.default.removeItem(at: r.folder.url) }
        #expect(r.folder.markerExists, "the app wrote its marker into \(r.folder.url.path)")
        #expect(r.closedUnexpectedly && r.showsBanner, "the planted marker was found: the banner shows")
        #expect(r.folder.reports().contains { $0.stem.hasPrefix("crash-session-") })
    }
    #endif

    /// The banner view reads `showsBanner` in its body; Dismiss must be a change that read observes (so the overlay
    /// redraws without it), and nothing at launch or a later refresh raises it again in the same run.
    @Test func dismissIsObservedByTheBannerAndStaysDismissed() throws {
        let folder = tempFolder()
        defer { try? FileManager.default.removeItem(at: folder.url) }
        folder.writeMarker(now: Date().addingTimeInterval(-60))      // the last run did not end normally
        let r = CrashReporter(folder: folder)
        r.start(register: false)
        #expect(r.showsBanner)
        final class Flag: @unchecked Sendable { var set = false }   // onChange runs synchronously, in dismiss()
        let changed = Flag()
        withObservationTracking { _ = r.showsBanner } onChange: { changed.set = true }
        r.dismiss()
        #expect(changed.set, "the banner's read of showsBanner sees Dismiss")
        #expect(!r.showsBanner)
        r.refresh()
        r.record(metrics: [.init(kind: "hang", json: Data("{}".utf8))])
        #expect(!r.showsBanner, "a dismissed banner stays down for the rest of the run")
    }

    /// A click on the banner's Dismiss reaches its action. The orange border was an overlay shape over the whole strip,
    /// and a shape takes the clicks inside it: every button under it was dead (the UI test's Dismiss click did nothing,
    /// and no `seen.marker` was written). The click goes through AppKit (`NSWindow.sendEvent`), as a user's does.
    @Test func aClickOnDismissReachesItsAction() throws {
        let folder = tempFolder()
        defer { try? FileManager.default.removeItem(at: folder.url) }
        folder.writeMarker(now: Date().addingTimeInterval(-60))
        let r = CrashReporter(folder: folder)
        r.start(register: false)
        #expect(r.showsBanner)
        let banner = CrashBanner(reporter: r, model: { nil })
        let size = NSHostingView(rootView: banner).fittingSize
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = NSHostingView(rootView: banner.frame(width: size.width, height: size.height, alignment: .top))
        window.orderBack(nil)
        defer { window.orderOut(nil) }
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        // Dismiss is the strip's last button: its middle is ~35 pt in from the 14 pt padding; the strip's 8 pt top
        // margin is above it (window points run from the bottom)
        let p = NSPoint(x: size.width - 14 - 35, y: (size.height - 8) / 2)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let e = try #require(NSEvent.mouseEvent(with: type, location: p, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                                    windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
            window.sendEvent(e)
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        }
        RunLoop.main.run(until: Date().addingTimeInterval(0.2))
        #expect(!r.showsBanner, "Dismiss ran")
        #expect(FileManager.default.fileExists(atPath: folder.seenURL.path), "and marked the reports seen")
    }

    @Test func aUnitTestHostWithoutAFolderDoesNotStart() {
        #expect(!CrashReporter.shouldStart(environment: ["XCTestConfigurationFilePath": "/x"], defaults: UserDefaults(suiteName: "sf-none-\(UUID().uuidString)")!))
        #expect(CrashReporter.shouldStart(environment: [:]))
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
