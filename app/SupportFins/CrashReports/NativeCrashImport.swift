import Foundation
import SupportFinsCore

/// The next launch's half of native crash capture (fix plan 1E): each raw dump the signal or exception handler left
/// in the crash folder (`native-*.sfdump`, SupportFinsCore's `NativeCrashCapture`) becomes a readable report there,
/// `crash-native-<time>.json` and `.txt`, and the raw dump goes. The report flows through the banner and Help >
/// Show crash reports like the others.
///
/// - Frames are named against the images of this launch with the same UUID (dladdr), else the image's file when it is
///   still there with that UUID; the rest keep image + offset (`NativeSymbolizer`).
/// - The breadcrumbs and settings are the dump's own (the handler wrote the latest); a dump without them takes the
///   last saved ones.
/// - A Rust panic that aborted (its stack runs through the panic machinery) already has the engine's report
///   (`crash-rust-*`, written by the panic hook just before the abort): the native stack joins that report, so the
///   crash gives one report, not two.
enum NativeCrashImport {
    /// How long before the abort the panic hook's report may have been written.
    static let rustReportWindow: TimeInterval = 120

    /// Turn every raw dump in `folder` into a report. Returns the stems written or joined, oldest dump first.
    @discardableResult
    static func run(folder: CrashFolder, fallback: (breadcrumbs: [String], settings: Any), now: Date = Date(),
                    loaded: [String: UInt] = NativeSymbolizer.loadedImages()) -> [String] {
        var stems: [String] = []
        for url in NativeCrashCapture.dumps(in: folder.url) {
            defer { try? FileManager.default.removeItem(at: url) }
            guard let data = try? Data(contentsOf: url), let dump = NativeCrashDump.parse(data) else { continue }
            if let stem = importOne(dump, folder: folder, fallback: fallback, now: now, loaded: loaded) { stems.append(stem) }
        }
        return stems
    }

    private static func importOne(_ dump: NativeCrashDump, folder: CrashFolder, fallback: (breadcrumbs: [String], settings: Any),
                                  now: Date, loaded: [String: UInt]) -> String? {
        let symbolizer = NativeSymbolizer(images: dump.images, loaded: loaded)
        let made = NativeCrashReport.make(dump, symbolizer: symbolizer)
        let context = dump.contextObject
        let crumbs = context?["breadcrumbs"] as? [String] ?? fallback.breadcrumbs
        let settings = context.map { $0["settings"] ?? NSNull() } ?? fallback.settings

        let signalFrames = dump.signal.map(symbolizer.frames(of:)) ?? []
        if NativeCrashReport.isRustPanicAbort(signal: dump.signal?.signal, frames: signalFrames),
           let rust = rustReport(in: folder, near: dump.time) {
            return join(rust, native: made, folder: folder) ? rust.stem : nil
        }

        let iso = ISO8601DateFormatter()
        var json = made.json
        let kind = dump.exception != nil ? "native-exception" : "native-signal"
        json["kind"] = kind
        json["time"] = iso.string(from: now)
        json["appVersion"] = dump.appVersion
        json["os"] = ProcessInfo.processInfo.operatingSystemVersionString
        json["breadcrumbs"] = crumbs
        json["settings"] = settings
        var text = "\(title(dump))\n"
        text += "crashTime: \(iso.string(from: dump.time))\nappVersion: \(dump.appVersion)\nos: \(ProcessInfo.processInfo.operatingSystemVersionString)\n\n"
        text += made.text
        text += "\nLast actions (oldest first):\n" + crumbs.map { "  \($0)\n" }.joined()
        if !(settings is NSNull), let d = try? JSONSerialization.data(withJSONObject: settings, options: [.prettyPrinted, .sortedKeys, .fragmentsAllowed]) {
            text += "\nSettings:\n" + String(decoding: d, as: UTF8.self) + "\n"
        }
        return folder.write(kind: "native", json: json, text: text, now: now)
    }

    static func title(_ dump: NativeCrashDump) -> String {
        if let e = dump.exception { return "Support Fins crashed: uncaught exception \(e.exceptionName ?? "?")" }
        let sig = dump.signal?.signal ?? 0
        return "Support Fins crashed (\(NativeCrashReport.signalName(sig)))"
    }

    /// The engine's panic report written just before the abort: the newest `crash-rust-*` from the window before it.
    private static func rustReport(in folder: CrashFolder, near time: Date) -> CrashFolder.Report? {
        folder.reports().first { r in
            r.stem.hasPrefix("crash-rust-") && r.date <= time.addingTimeInterval(10) && r.date >= time.addingTimeInterval(-rustReportWindow)
        }
    }

    /// Add the native capture to the panic report: `native` in its JSON, the stacks at the end of its text.
    private static func join(_ report: CrashFolder.Report, native: (json: [String: Any], text: String, frames: [NativeSymbolizer.Frame]), folder: CrashFolder) -> Bool {
        guard let jsonURL = report.files.first(where: { $0.pathExtension == "json" }),
              let data = try? Data(contentsOf: jsonURL),
              var object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return false }
        object["native"] = native.json
        object["abortedTheApp"] = true
        guard let out = try? JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys]) else { return false }
        try? out.write(to: jsonURL)
        if let textURL = report.files.first(where: { $0.pathExtension == "txt" }), let handle = try? FileHandle(forWritingTo: textURL) {
            handle.seekToEndOfFile()
            handle.write(Data("\nThe panic aborted the app. Native capture at the abort:\n\n\(native.text)".utf8))
            try? handle.close()
        }
        return true
    }

    /// The native report of the same crash as a MetricKit crash diagnostic: its crash time in the payload's window
    /// (a little slack either side) and, when both know it, the same signal. Nil when there is none.
    static func nativeReport(in folder: CrashFolder, begin: Date?, end: Date?, signal: Int?) -> String? {
        guard let begin, let end else { return nil }
        let iso = ISO8601DateFormatter()
        for r in folder.reports() where r.stem.hasPrefix("crash-native-") || r.stem.hasPrefix("crash-rust-") {
            guard let f = r.files.first(where: { $0.pathExtension == "json" }), let data = try? Data(contentsOf: f),
                  let o = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { continue }
            let native = r.stem.hasPrefix("crash-native-") ? o : o["native"] as? [String: Any]
            guard let native, let t = (native["crashTime"] as? String).flatMap(iso.date(from:)),
                  t >= begin.addingTimeInterval(-60), t <= end.addingTimeInterval(60) else { continue }
            let theirs = (native["signal"] as? [String: Any])?["signal"] as? Int
            if let signal, let theirs, signal != theirs { continue }
            return r.stem
        }
        return nil
    }
}
