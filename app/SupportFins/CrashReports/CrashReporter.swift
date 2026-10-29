import AppKit
import MetricKit
import Observation
import SupportFinsCore

/// Crash reports, the app's side (docs/FIX-PLAN-2026-10.md, 1D). The engine's panic hook writes its own reports into
/// the same folder (`crates/sf-ffi/src/crash.rs`); this class
/// - keeps the last 50 user actions (breadcrumbs) and saves them from time to time, so they survive a crash;
/// - sets a "running" marker at launch and clears it at normal termination: a marker at the next launch means the last
///   run did not end normally, and a report is written from the saved breadcrumbs;
/// - writes the MetricKit crash and hang diagnostics the system delivers on a later launch;
/// - tells the window whether to show the banner (a marker, or a report newer than the last Dismiss);
/// - prunes the folder at launch (20 reports or 30 days).
///
/// Nothing is sent anywhere; no network API is used.
@MainActor
@Observable
final class CrashReporter {
    static let shared = CrashReporter()

    @ObservationIgnored private(set) var folder: CrashFolder
    @ObservationIgnored let breadcrumbs = Breadcrumbs()
    @ObservationIgnored private var settingsJSON = "{}"
    @ObservationIgnored private var started = false
    @ObservationIgnored private var saveScheduled = false
    @ObservationIgnored private var subscriber: MetricsSubscriber?

    /// The last run did not end normally (the marker was there at launch).
    private(set) var closedUnexpectedly = false
    /// Reports written since the last Dismiss.
    private(set) var unseen: [CrashFolder.Report] = []
    private var dismissed = false

    init(folder: CrashFolder = CrashFolder(url: CrashFolder.standard)) { self.folder = folder }

    /// The banner: the last run ended badly, or a report is new, and the user has not dismissed it.
    var showsBanner: Bool { !dismissed && (closedUnexpectedly || !unseen.isEmpty) }

    // MARK: launch and termination

    /// Call once at launch. `register` is false in tests that only exercise the logic.
    func start(register: Bool = true) {
        guard !started else { return }
        started = true
        #if DEBUG
        let args = UserDefaults.standard
        // test hooks, DEBUG builds only: a folder of the test's own, and a marker planted as if the last run crashed
        if let dir = args.string(forKey: "SFCrashDir") { folder = CrashFolder(url: URL(fileURLWithPath: dir, isDirectory: true)) }
        if args.bool(forKey: "SFPlantCrash") {
            folder.create()
            folder.writeMarker(now: Date().addingTimeInterval(-60))
            breadcrumbs.add("open file: planted.stl (STL), 1 objects, 12 triangles")
            saveNow()
        }
        #endif
        folder.create()
        folder.prune()
        if folder.markerExists { recordUnexpectedExit() }
        folder.writeMarker()
        breadcrumbs.replace(with: [])
        guard register else { refresh(); return }
        crashReportsStart(dir: folder.url.path, appVersion: Self.appVersion, settingsJson: settingsJSON)
        let sub = MetricsSubscriber { [weak self] items in Task { @MainActor in self?.record(metrics: items) } }
        subscriber = sub
        MXMetricManager.shared.add(sub)
        NotificationCenter.default.addObserver(forName: NSApplication.willTerminateNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { CrashReporter.shared.stop() }
        }
        refresh()
    }

    /// Normal termination: the marker goes, the breadcrumbs of this run are saved.
    func stop() {
        saveNow()
        folder.clearMarker()
    }

    static var appVersion: String {
        let info = Bundle.main.infoDictionary
        return "\(info?["CFBundleShortVersionString"] as? String ?? "?") (\(info?["CFBundleVersion"] as? String ?? "?"))"
    }

    // MARK: breadcrumbs and settings

    /// One user action. Cheap: a lock, a line and a push into the engine's ring; the file is saved a moment later.
    static func note(_ text: String) { shared.note(text) }

    func note(_ text: String) {
        breadcrumbs.add(text)
        if started { crashBreadcrumb(text: text) }
        scheduleSave()
    }

    /// The options and tunables texts of the build now in force: they go into every report.
    static func setSettings(options: String, tunables: String) { shared.setSettings(options: options, tunables: tunables) }

    func setSettings(options: String, tunables: String) {
        let json = "{\"options\":\(options.isEmpty ? "null" : options),\"tunables\":\(tunables.isEmpty ? "null" : tunables)}"
        guard json != settingsJSON else { return }
        settingsJSON = json
        if started { crashSetSettings(settingsJson: json) }
        scheduleSave()
    }

    private func scheduleSave() {
        guard started, !saveScheduled else { return }
        saveScheduled = true
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(2))
            self?.saveScheduled = false
            self?.saveNow()
        }
    }

    /// Write the breadcrumbs and settings of this run to the folder.
    func saveNow() {
        let object: [String: Any] = ["savedAt": ISO8601DateFormatter().string(from: Date()), "breadcrumbs": breadcrumbs.lines, "settings": Self.object(settingsJSON)]
        guard let data = try? JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys]) else { return }
        try? data.write(to: folder.breadcrumbsURL)
    }

    private static func object(_ text: String) -> Any {
        (try? JSONSerialization.jsonObject(with: Data(text.utf8), options: [.fragmentsAllowed])) ?? text
    }

    // MARK: the run that did not end normally

    /// The saved breadcrumbs and settings of the last run (a missing file: none).
    private func savedSession() -> (breadcrumbs: [String], settings: Any) {
        guard let data = try? Data(contentsOf: folder.breadcrumbsURL),
              let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return ([], NSNull()) }
        return (o["breadcrumbs"] as? [String] ?? [], o["settings"] ?? NSNull())
    }

    /// The marker was there at launch: keep the last run's breadcrumbs and write a report from them. MetricKit's
    /// diagnostics for the same crash arrive later, if at all, and carry the same breadcrumbs.
    private func recordUnexpectedExit(now: Date = Date()) {
        closedUnexpectedly = true
        let saved = savedSession()
        try? FileManager.default.removeItem(at: folder.crashedBreadcrumbsURL)
        try? FileManager.default.moveItem(at: folder.breadcrumbsURL, to: folder.crashedBreadcrumbsURL)
        let iso = ISO8601DateFormatter()
        let started = folder.markerDate().map(iso.string(from:)) ?? "unknown"
        let json: [String: Any] = [
            "kind": "unexpected-exit", "time": iso.string(from: now), "runStartedAt": started,
            "appVersion": Self.appVersion, "engineVersion": engineVersion(), "os": ProcessInfo.processInfo.operatingSystemVersionString,
            "breadcrumbs": saved.breadcrumbs, "settings": saved.settings,
        ]
        let text = Self.text(title: "Support Fins closed unexpectedly (no normal quit)", json: json, crumbs: saved.breadcrumbs)
        folder.write(kind: "session", json: json, text: text, now: now)
    }

    private static func text(title: String, json: [String: Any], crumbs: [String]) -> String {
        var out = "\(title)\n"
        for k in ["time", "runStartedAt", "appVersion", "engineVersion", "os"] { if let v = json[k] { out += "\(k): \(v)\n" } }
        out += "\nLast actions (oldest first):\n" + crumbs.map { "  \($0)\n" }.joined()
        if let s = json["settings"], !(s is NSNull),
           let d = try? JSONSerialization.data(withJSONObject: s, options: [.prettyPrinted, .sortedKeys, .fragmentsAllowed]) {
            out += "\nSettings:\n" + String(decoding: d, as: UTF8.self) + "\n"
        }
        if let d = json["diagnostic"], let data = try? JSONSerialization.data(withJSONObject: d, options: [.prettyPrinted, .sortedKeys]) {
            out += "\nDiagnostic:\n" + String(decoding: data, as: UTF8.self) + "\n"
        }
        return out
    }

    // MARK: MetricKit

    /// Write each crash and hang diagnostic the system delivered, with the breadcrumbs of the run that went wrong
    /// (the crashed run's if there was one, else the last saved).
    func record(metrics items: [MetricsSubscriber.Item], now: Date = Date()) {
        guard !items.isEmpty else { return }
        let crumbs: [String]
        let settings: Any
        if let data = try? Data(contentsOf: folder.crashedBreadcrumbsURL), let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            (crumbs, settings) = (o["breadcrumbs"] as? [String] ?? [], o["settings"] ?? NSNull())
        } else {
            (crumbs, settings) = (breadcrumbs.lines, Self.object(settingsJSON))
        }
        let iso = ISO8601DateFormatter()
        for item in items {
            let diagnostic = (try? JSONSerialization.jsonObject(with: item.json)) ?? NSNull()
            let json: [String: Any] = [
                "kind": "metrickit-\(item.kind)", "time": iso.string(from: now), "appVersion": Self.appVersion,
                "engineVersion": engineVersion(), "os": ProcessInfo.processInfo.operatingSystemVersionString,
                "breadcrumbs": crumbs, "settings": settings, "diagnostic": diagnostic,
            ]
            folder.write(kind: "metrickit-\(item.kind)", json: json, text: Self.text(title: "MetricKit \(item.kind) diagnostic", json: json, crumbs: crumbs), now: now)
        }
        refresh()
    }

    // MARK: the banner

    /// Look at the folder again (a report just landed).
    func refresh() { unseen = folder.unseen() }

    /// Dismiss: the reports now in the folder are seen, and the marker's news is old.
    func dismiss() {
        folder.markSeen()
        dismissed = true
        closedUnexpectedly = false
        unseen = []
    }

    /// The report files the banner talks about: the unseen reports (the newest one when all are seen).
    var bannerFiles: [URL] {
        let list = unseen.isEmpty ? Array(folder.reports().prefix(1)) : unseen
        return list.flatMap(\.files)
    }

    /// Show Report: the newest report in Finder.
    func revealReport() {
        let files = Array(bannerFiles.filter { $0.pathExtension == "txt" }.prefix(1))
        NSWorkspace.shared.activateFileViewerSelecting(files.isEmpty ? bannerFiles : files)
    }

    /// Help > Show crash reports: the folder in Finder.
    func revealFolder() {
        folder.create()
        NSWorkspace.shared.open(folder.url)
    }

    /// Save crash report…: asks where, offers the model file (off by default) and writes the zip.
    func saveReport(model: URL?) {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.zip]
        panel.nameFieldStringValue = "Support Fins crash report.zip"
        let box = NSButton(checkboxWithTitle: JS.t("Include the model file that was open"), target: nil, action: nil)
        box.state = .off
        box.isEnabled = model != nil
        box.setAccessibilityIdentifier("crash-include-model")
        panel.accessoryView = box
        guard panel.runModal() == .OK, let out = panel.url else { return }
        let extras = [folder.crashedBreadcrumbsURL].filter { FileManager.default.fileExists(atPath: $0.path) }
        if let data = try? CrashArchive.make(reports: bannerFiles, extras: extras, model: box.state == .on ? model : nil) {
            try? data.write(to: out)
        }
    }
}

/// Receives the system's diagnostics (on a queue of its own) and hands over each crash and hang as JSON.
final class MetricsSubscriber: NSObject, MXMetricManagerSubscriber, @unchecked Sendable {
    struct Item: Sendable {
        let kind: String
        let json: Data
    }

    private let deliver: @Sendable ([Item]) -> Void

    init(deliver: @escaping @Sendable ([Item]) -> Void) { self.deliver = deliver }

    func didReceive(_ payloads: [MXDiagnosticPayload]) {
        var items: [Item] = []
        for p in payloads {
            items += (p.crashDiagnostics ?? []).map { Item(kind: "crash", json: $0.jsonRepresentation()) }
            items += (p.hangDiagnostics ?? []).map { Item(kind: "hang", json: $0.jsonRepresentation()) }
        }
        deliver(items)
    }
}
