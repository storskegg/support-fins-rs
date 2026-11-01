import AppKit
import MetricKit
import Observation
import SupportFinsCore

/// Crash reports, the app's side (docs/FIX-PLAN-2026-10.md, 1D). The engine's panic hook writes its own reports into
/// the same folder (`crates/sf-ffi/src/crash.rs`); this class
/// - keeps the last 50 user actions (breadcrumbs) and saves them as they go (at launch, then within a quarter second of
///   each action), so they survive a crash or a kill: a run killed outright (SIGKILL: memory pressure, a force quit) has
///   no handler to save them then (S6, November 2026);
/// - keeps the last build or analysis stage and the process's memory footprint (last and peak; sampled every few
///   seconds and at each stage) in the same file, so a report says what the app was doing and how big it was;
/// - sets a "running" marker at launch and clears it at normal termination: a marker at the next launch means the last
///   run did not end normally, and a report is written from the saved breadcrumbs;
/// - writes the MetricKit crash and hang diagnostics the system delivers on a later launch;
/// - tells the window whether to show the banner (a marker, or a report newer than the last Dismiss);
/// - prunes the folder at launch (20 reports or 30 days);
/// - installs the native crash handlers (1E, SupportFinsCore's `NativeCrashCapture`): a fatal signal or an uncaught
///   NSException writes a raw dump with the crashing thread's stack, the images and the latest breadcrumbs and settings
///   at the moment of the crash, and the next launch turns it into a report (`NativeCrashImport`).
///
/// Tests never write into the user's folder and never raise the banner from a native dump: a unit-test host and a
/// UI-test run (DEBUG `-SFCrashDir`) write their raw dumps into a per-run folder in the process's own temporary
/// directory (`nativeTestFolder`), printed to stderr at install, and nothing converts them; `just app-test` fails when
/// one appears (tools/gate/native_dumps.py).
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
    @ObservationIgnored private var lastSave: ContinuousClock.Instant?
    /// The last build or analysis stage (`stage(_:)`), `<ISO time>  text`; not a breadcrumb (they come many a second).
    @ObservationIgnored private(set) var lastStage: String?
    /// The memory footprint: the last sample, the highest seen this run, and when the last was taken.
    struct Memory: Equatable {
        var lastMB: Int
        var peakMB: Int
        var at: Date
    }
    @ObservationIgnored private(set) var memory: Memory?
    @ObservationIgnored private var sampler: Task<Void, Never>?
    /// How soon after an action the file is written (the first one at once).
    static let saveDelay = Duration.milliseconds(250)
    /// How often the memory is sampled while the app runs; a move of `memoryStepMB` or a new peak saves the file.
    static let memoryInterval = Duration.seconds(5)
    static let memoryStepMB = 256
    @ObservationIgnored private var subscriber: MetricsSubscriber?
    /// The native handlers are installed in this process (by any reporter, or the test-host capture).
    private static var nativeInstalled = false

    /// The last run did not end normally (the marker was there at launch).
    private(set) var closedUnexpectedly = false
    /// Reports written since the last Dismiss.
    private(set) var unseen: [CrashFolder.Report] = []
    private var dismissed = false

    init(folder: CrashFolder = CrashFolder(url: CrashFolder.standard)) { self.folder = folder }

    /// The banner: the last run ended badly, or a report is new, and the user has not dismissed it.
    /// A caught Rust panic (`crash-rust-*`) is a report but not a close: the app went on, so it never raises the banner.
    var showsBanner: Bool { !dismissed && (closedUnexpectedly || !unseenCloses.isEmpty) }

    /// The unseen reports of a run that ended: the launch's own and MetricKit's crashes and hangs.
    private var unseenCloses: [CrashFolder.Report] { unseen.filter { !$0.stem.hasPrefix("crash-rust-") } }

    // MARK: launch and termination

    /// False in a unit-test host (it would write markers into the user's folder). A DEBUG build launched with its own
    /// `-SFCrashDir` (the UI tests) starts, whatever the environment says.
    static func shouldStart(environment: [String: String], defaults: UserDefaults = AppDefaults.shared) -> Bool {
        #if DEBUG
        if defaults.string(forKey: "SFCrashDir") != nil { return true }
        #endif
        return environment["XCTestConfigurationFilePath"] == nil
    }

    /// Where a test's raw native dumps go: `<temp>/sf-native-crash/<run>/` in the process's own temporary directory
    /// (the app's sandbox container for the app and its unit-test host). `<run>` is `SF_CRASH_RUN` when set (`just
    /// app-test` passes it as `TEST_RUNNER_SF_CRASH_RUN`), else the launch time and pid. Nil outside tests.
    static func nativeTestFolder(environment: [String: String], defaults: UserDefaults = AppDefaults.shared) -> URL? {
        var testing = environment["XCTestConfigurationFilePath"] != nil
        #if DEBUG
        if defaults.string(forKey: "SFCrashDir") != nil { testing = true }
        #endif
        guard testing else { return nil }
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyyMMdd-HHmmss"
        let run = environment["SF_CRASH_RUN"].map { $0.split(separator: "/").filter { $0 != ".." }.joined(separator: "_") }
            ?? "\(f.string(from: Date()))-\(getpid())"
        return FileManager.default.temporaryDirectory.appending(path: "sf-native-crash/\(run)", directoryHint: .isDirectory)
    }

    /// A unit-test host (the reporter does not start there): only the native handlers, into the run's test folder.
    static func startTestCapture(environment: [String: String] = ProcessInfo.processInfo.environment) {
        guard let dir = nativeTestFolder(environment: environment) else { return }
        installNative(dir: dir, testing: true)
        shared.pushNativeContext()
    }

    private static func installNative(dir: URL, testing: Bool) {
        guard NativeCrashCapture.install(dir: dir, appVersion: appVersion) else { return }
        nativeInstalled = true
        if testing {
            FileHandle.standardError.write(Data("Support Fins: native crash dumps of this test run go to \(dir.path)\n".utf8))
        }
    }

    /// Call once at launch. `register` is false in tests that only exercise the logic.
    func start(register: Bool = true) {
        guard !started else { return }
        started = true
        #if DEBUG
        let args = AppDefaults.shared
        // test hooks, DEBUG builds only: a folder of the test's own, and a marker planted as if the last run crashed
        if let dir = args.string(forKey: "SFCrashDir") { folder = CrashFolder(url: CrashFolder.testFolder(named: dir)) }
        if args.bool(forKey: "SFPlantCrash") {
            folder.create()
            folder.writeMarker(now: Date().addingTimeInterval(-60))
            breadcrumbs.add("open file: planted.stl (STL), 1 objects, 12 triangles")
            saveNow()
        }
        #endif
        folder.create()
        folder.prune()
        // the raw dumps of a native crash first: one of them is the report of the run the marker speaks of
        let imported = NativeCrashImport.run(folder: folder, fallback: savedSession())
        if folder.markerExists { recordUnexpectedExit(writeReport: imported.isEmpty) }
        folder.writeMarker()
        breadcrumbs.replace(with: [])
        lastStage = nil
        memory = nil
        sampleMemory()
        // this run's file from the start: a run killed before its first action leaves its own (empty) file, not the
        // previous run's (the report of S6 carried breadcrumbs saved a day before)
        saveNow()
        guard register else { refresh(); return }
        crashReportsStart(dir: folder.url.path, appVersion: Self.appVersion, settingsJson: settingsJSON)
        let testDir = Self.nativeTestFolder(environment: ProcessInfo.processInfo.environment)
        Self.installNative(dir: testDir ?? folder.url, testing: testDir != nil)
        pushNativeContext()
        let sub = MetricsSubscriber { [weak self] items in Task { @MainActor in self?.record(metrics: items) } }
        subscriber = sub
        MXMetricManager.shared.add(sub)
        NotificationCenter.default.addObserver(forName: NSApplication.willTerminateNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { CrashReporter.shared.stop() }
        }
        startMemorySampler()
        refresh()
    }

    /// Samples the memory footprint every `memoryInterval`; a new peak, or a move of `memoryStepMB` since the last save,
    /// writes the file, so a run that grows until the system kills it leaves how big it got.
    private func startMemorySampler() {
        sampler?.cancel()
        sampler = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: Self.memoryInterval)
                guard let self else { return }
                let before = self.savedMemory
                self.sampleMemory()
                if let m = self.memory, before == nil || m.peakMB > before!.peakMB || abs(m.lastMB - before!.lastMB) >= Self.memoryStepMB {
                    self.scheduleSave()
                }
            }
        }
    }

    /// The memory as last written to the file.
    @ObservationIgnored private var savedMemory: Memory?

    /// Takes a memory sample now.
    func sampleMemory(now: Date = Date()) {
        guard let bytes = MemoryFootprint.current() else { return }
        let mb = MemoryFootprint.megabytes(bytes)
        memory = Memory(lastMB: mb, peakMB: max(mb, memory?.peakMB ?? 0), at: now)
    }

    /// Normal termination: the marker goes, the breadcrumbs of this run are saved.
    func stop() {
        sampler?.cancel()
        sampler = nil
        saveNow()
        folder.clearMarker()
    }

    /// "2026.44.0 (1234)": the version and the build number (`AppVersion`).
    static var appVersion: String { AppVersion.display }

    // MARK: breadcrumbs and settings

    /// One user action. Cheap: a lock, a line and a push into the engine's ring; the file is saved a moment later.
    static func note(_ text: String) { shared.note(text) }

    func note(_ text: String) {
        breadcrumbs.add(text)
        if started { crashBreadcrumb(text: text) }
        pushNativeContext()
        scheduleSave()
    }

    /// The stage the app is in now: a build or an analysis starting or ending, a file being read or merged. Kept apart
    /// from the breadcrumbs (a drag makes many a second), with a memory sample, and saved like them.
    static func stage(_ text: String) { shared.stage(text) }

    func stage(_ text: String, now: Date = Date()) {
        lastStage = "\(now.formatted(Date.ISO8601FormatStyle(includingFractionalSeconds: true)))  \(text)"
        sampleMemory(now: now)
        pushNativeContext()
        scheduleSave()
    }

    /// The breadcrumbs and settings a native crash writes, preserialized for the handler; the stage and the memory
    /// ride as the last two lines.
    private func pushNativeContext() {
        guard Self.nativeInstalled else { return }
        NativeCrashCapture.setContext(breadcrumbs: breadcrumbs.lines + contextLines, settingsJSON: settingsJSON)
    }

    /// The stage and the memory as report lines.
    private var contextLines: [String] {
        (lastStage.map { ["stage: \($0)"] } ?? []) + (memory.map { ["memory: \(Self.memoryText($0))"] } ?? [])
    }

    static func memoryText(_ m: Memory) -> String {
        "\(m.lastMB) MB (peak \(m.peakMB) MB) at \(ISO8601DateFormatter().string(from: m.at))"
    }

    /// The options and tunables texts of the build now in force: they go into every report.
    static func setSettings(options: String, tunables: String) { shared.setSettings(options: options, tunables: tunables) }

    func setSettings(options: String, tunables: String) {
        let json = "{\"options\":\(options.isEmpty ? "null" : options),\"tunables\":\(tunables.isEmpty ? "null" : tunables)}"
        guard json != settingsJSON else { return }
        settingsJSON = json
        if started { crashSetSettings(settingsJson: json) }
        pushNativeContext()
        scheduleSave()
    }

    /// Writes the file now when the last write is older than `saveDelay`, else once that much after it: an action is
    /// on disk a quarter second after it at most, and a burst of them is one write each quarter second.
    private func scheduleSave() {
        guard started, !saveScheduled else { return }
        let clock = ContinuousClock()
        if let last = lastSave, clock.now - last < Self.saveDelay {
            saveScheduled = true
            let wait = Self.saveDelay - (clock.now - last)
            Task { @MainActor [weak self] in
                try? await Task.sleep(for: wait)
                self?.saveScheduled = false
                self?.saveNow()
            }
            return
        }
        saveNow()
    }

    /// Write the breadcrumbs and settings of this run to the folder.
    func saveNow() {
        lastSave = ContinuousClock().now
        savedMemory = memory
        var object: [String: Any] = ["savedAt": ISO8601DateFormatter().string(from: Date()), "breadcrumbs": breadcrumbs.lines, "settings": Self.object(settingsJSON)]
        if let lastStage { object["lastStage"] = lastStage }
        if let memory {
            object["memory"] = ["lastMB": memory.lastMB, "peakMB": memory.peakMB, "at": ISO8601DateFormatter().string(from: memory.at)] as [String: Any]
        }
        guard let data = try? JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys]) else { return }
        try? data.write(to: folder.breadcrumbsURL)
    }

    private static func object(_ text: String) -> Any {
        (try? JSONSerialization.jsonObject(with: Data(text.utf8), options: [.fragmentsAllowed])) ?? text
    }

    // MARK: the run that did not end normally

    /// The saved breadcrumbs and settings of the last run (a missing file: none).
    private func savedSession() -> (breadcrumbs: [String], settings: Any) {
        let s = savedFile()
        return (s?["breadcrumbs"] as? [String] ?? [], s?["settings"] ?? NSNull())
    }

    private func savedFile() -> [String: Any]? {
        guard let data = try? Data(contentsOf: folder.breadcrumbsURL) else { return nil }
        return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    }

    /// The marker was there at launch: keep the last run's breadcrumbs and write a report from them. MetricKit's
    /// diagnostics for the same crash arrive later, if at all, and carry the same breadcrumbs. `writeReport` is false
    /// when a native dump of that run became its report already.
    private func recordUnexpectedExit(writeReport: Bool = true, now: Date = Date()) {
        closedUnexpectedly = true
        let saved = savedSession()
        let file = savedFile()
        try? FileManager.default.removeItem(at: folder.crashedBreadcrumbsURL)
        try? FileManager.default.moveItem(at: folder.breadcrumbsURL, to: folder.crashedBreadcrumbsURL)
        guard writeReport else { return }
        let iso = ISO8601DateFormatter()
        let started = folder.markerDate().map(iso.string(from:)) ?? "unknown"
        var json: [String: Any] = [
            "kind": "unexpected-exit", "time": iso.string(from: now), "runStartedAt": started,
            "appVersion": Self.appVersion, "engineVersion": engineVersion(), "os": ProcessInfo.processInfo.operatingSystemVersionString,
            "breadcrumbs": saved.breadcrumbs, "settings": saved.settings,
        ]
        // what the run was doing and how big it was when its file was last written (the exit came after that)
        if let stage = file?["lastStage"] { json["lastStage"] = stage }
        if let m = file?["memory"] { json["memory"] = m }
        if let at = file?["savedAt"] { json["lastSavedAt"] = at }
        let text = Self.text(title: "Support Fins closed unexpectedly (no normal quit)", json: json, crumbs: saved.breadcrumbs)
        folder.write(kind: "session", json: json, text: text, now: now)
    }

    private static func text(title: String, json: [String: Any], crumbs: [String]) -> String {
        var out = "\(title)\n"
        for k in ["time", "runStartedAt", "lastSavedAt", "appVersion", "engineVersion", "os", "lastStage"] { if let v = json[k] { out += "\(k): \(v)\n" } }
        if let m = json["memory"] as? [String: Any], let last = m["lastMB"], let peak = m["peakMB"] {
            out += "memory: \(last) MB (peak \(peak) MB) at \(m["at"] ?? "?")\n"
        }
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
            var json: [String: Any] = [
                "kind": "metrickit-\(item.kind)", "time": iso.string(from: now), "appVersion": Self.appVersion,
                "engineVersion": engineVersion(), "os": ProcessInfo.processInfo.operatingSystemVersionString,
                "breadcrumbs": crumbs, "settings": settings, "diagnostic": diagnostic,
            ]
            var title = "MetricKit \(item.kind) diagnostic"
            // the same crash, captured natively at the time: say so (this copy came a launch or more later)
            if item.kind == "crash", let native = NativeCrashImport.nativeReport(in: folder, begin: item.begin, end: item.end, signal: item.signal) {
                json["alsoCapturedNatively"] = native
                title += " (also captured natively: \(native))"
            }
            folder.write(kind: "metrickit-\(item.kind)", json: json, text: Self.text(title: title, json: json, crumbs: crumbs), now: now)
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
        let list = unseenCloses.isEmpty ? Array(folder.reports().prefix(1)) : unseenCloses
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
            try? AtomicWrite.write(data, to: out)
        }
    }
}

/// Receives the system's diagnostics (on a queue of its own) and hands over each crash and hang as JSON.
final class MetricsSubscriber: NSObject, MXMetricManagerSubscriber, @unchecked Sendable {
    struct Item: Sendable {
        let kind: String
        let json: Data
        /// The payload's time window and the crash's signal (crash diagnostics): to match a native report.
        var begin: Date? = nil
        var end: Date? = nil
        var signal: Int? = nil
    }

    private let deliver: @Sendable ([Item]) -> Void

    init(deliver: @escaping @Sendable ([Item]) -> Void) { self.deliver = deliver }

    func didReceive(_ payloads: [MXDiagnosticPayload]) {
        var items: [Item] = []
        for p in payloads {
            items += (p.crashDiagnostics ?? []).map {
                Item(kind: "crash", json: $0.jsonRepresentation(), begin: p.timeStampBegin, end: p.timeStampEnd, signal: $0.signal?.intValue)
            }
            items += (p.hangDiagnostics ?? []).map { Item(kind: "hang", json: $0.jsonRepresentation()) }
        }
        deliver(items)
    }
}
