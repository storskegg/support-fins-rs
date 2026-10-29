import Foundation

/// The folder crash reports live in (`Library/Logs/Support Fins/`, which is inside the sandbox container) and what
/// is kept there: reports, the last-session breadcrumbs, the "running" marker and the "seen" marker.
///
/// A report is one or two files with the same stem, `crash-<kind>-<time>.json` and `.txt`. The folder keeps the
/// newest `maxReports` reports for `maxAge`, whichever is fewer. Nothing here talks to the network.
struct CrashFolder: Sendable {
    static let maxReports = 20
    static let maxAge: TimeInterval = 30 * 24 * 3600
    static let prefix = "crash-"

    let url: URL

    /// `~/Library/Logs/Support Fins` (the container's `Library` in the sandbox).
    static var standard: URL {
        let library = FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        return library.appending(path: "Logs/Support Fins", directoryHint: .isDirectory)
    }

    var markerURL: URL { url.appending(path: "running.marker") }
    var seenURL: URL { url.appending(path: "seen.marker") }
    /// The breadcrumbs and settings of the session now running, saved from time to time.
    var breadcrumbsURL: URL { url.appending(path: "breadcrumbs.json") }
    /// The breadcrumbs of the last session, kept when that session did not end normally.
    var crashedBreadcrumbsURL: URL { url.appending(path: "breadcrumbs-crashed.json") }

    func create() {
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    }

    // MARK: markers

    var markerExists: Bool { FileManager.default.fileExists(atPath: markerURL.path) }

    func writeMarker(now: Date = Date()) {
        try? Data(ISO8601DateFormatter().string(from: now).utf8).write(to: markerURL)
    }

    func clearMarker() { try? FileManager.default.removeItem(at: markerURL) }

    /// When the marker was set (the launch that never ended normally).
    func markerDate() -> Date? {
        guard let text = try? String(contentsOf: markerURL, encoding: .utf8) else { return nil }
        return ISO8601DateFormatter().date(from: text.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    /// Dismiss: everything now in the folder has been seen.
    func markSeen(now: Date = Date()) {
        try? Data().write(to: seenURL)
        try? FileManager.default.setAttributes([.modificationDate: now], ofItemAtPath: seenURL.path)
    }

    var seenDate: Date? {
        (try? FileManager.default.attributesOfItem(atPath: seenURL.path))?[.modificationDate] as? Date
    }

    // MARK: reports

    struct Report: Identifiable, Equatable, Sendable {
        var id: String { stem }
        let stem: String
        let files: [URL]
        let date: Date
    }

    /// Every report in the folder, newest first.
    func reports() -> [Report] {
        let keys: [URLResourceKey] = [.contentModificationDateKey]
        let all = (try? FileManager.default.contentsOfDirectory(at: url, includingPropertiesForKeys: keys)) ?? []
        var byStem: [String: [URL]] = [:]
        for f in all where f.lastPathComponent.hasPrefix(Self.prefix) && ["json", "txt"].contains(f.pathExtension) {
            byStem[f.deletingPathExtension().lastPathComponent, default: []].append(f)
        }
        return byStem.map { stem, files in
            let date = files.compactMap { try? $0.resourceValues(forKeys: Set(keys)).contentModificationDate }.max() ?? .distantPast
            return Report(stem: stem, files: files.sorted { $0.lastPathComponent < $1.lastPathComponent }, date: date)
        }.sorted { ($0.date, $0.stem) > ($1.date, $1.stem) }
    }

    /// The reports written since the last Dismiss.
    func unseen() -> [Report] {
        let seen = seenDate
        return reports().filter { seen == nil || $0.date > seen! }
    }

    /// Keep the newest `maxReports` reports of at most `maxAge`; returns how many reports went.
    @discardableResult
    func prune(now: Date = Date()) -> Int {
        var removed = 0
        for (i, r) in reports().enumerated() where i >= Self.maxReports || now.timeIntervalSince(r.date) > Self.maxAge {
            r.files.forEach { try? FileManager.default.removeItem(at: $0) }
            removed += 1
        }
        return removed
    }

    // MARK: writing

    /// Write a report as `<stem>.json` and `<stem>.txt`; returns the stem. `stem` gets the time and a short suffix so
    /// two reports of one second do not meet.
    @discardableResult
    func write(kind: String, json: [String: Any], text: String, now: Date = Date()) -> String? {
        create()
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.timeZone = TimeZone(identifier: "UTC")
        f.dateFormat = "yyyy-MM-dd'T'HH-mm-ss"
        let stem = "\(Self.prefix)\(kind)-\(f.string(from: now))-\(UUID().uuidString.prefix(4))"
        guard JSONSerialization.isValidJSONObject(json),
              let data = try? JSONSerialization.data(withJSONObject: json, options: [.prettyPrinted, .sortedKeys])
        else { return nil }
        do {
            try data.write(to: url.appending(path: "\(stem).json"))
            try Data(text.utf8).write(to: url.appending(path: "\(stem).txt"))
        } catch { return nil }
        return stem
    }
}
