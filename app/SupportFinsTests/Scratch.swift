import Foundation

/// Scratch files for tests. Swift Testing runs tests in parallel, so a fixed path under the temp directory is
/// shared state: one test rewrites a file while another reads it. Every call gets its own directory.
///
/// Everything a test writes goes under this run's folder, `<temp>/sf-app-tests/<pid>-<uuid>/` (`root`), which is removed
/// when the test host exits: the host is sandboxed, so its temp is the app container's `Data/tmp`, and leaving one
/// folder per call there piled up 36,500 of them (user suggestion S7). A run that never exits normally (killed,
/// crashed) leaves its folder: the next run removes run folders older than a day (`sweep`), and so does the gate's
/// app-test step before it starts (tools/gate/test_tmp_sweep.py).
enum Scratch {
    /// Run folders older than this are a run that did not clean up after itself.
    static let staleAge: TimeInterval = 24 * 3600

    /// The parent of every run's folder.
    static let base = FileManager.default.temporaryDirectory.appending(path: "sf-app-tests", directoryHint: .isDirectory)

    /// This test run's folder, made on first use; removed at exit.
    static let root: URL = {
        sweep(base, olderThan: staleAge)
        let dir = base.appending(path: "\(getpid())-\(UUID().uuidString)", directoryHint: .isDirectory)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        atexit { Scratch.removeRoot() }
        return dir
    }()

    /// A new, empty directory in this run's folder; `remove` it when the test is done, or leave it to the run's clean-up.
    static func directory() throws -> URL {
        let dir = root.appending(path: UUID().uuidString, directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// `name` in a directory of its own: a fixed file name (the part's name shows in the export's file name) at
    /// a path nobody else writes.
    static func file(_ name: String) throws -> URL {
        try directory().appending(path: name)
    }

    static func remove(_ url: URL) { try? FileManager.default.removeItem(at: url) }

    /// Remove the entries of `folder` last modified more than `age` ago (another run's folder, left behind).
    static func sweep(_ folder: URL, olderThan age: TimeInterval, now: Date = Date()) {
        let fm = FileManager.default
        let entries = (try? fm.contentsOfDirectory(at: folder, includingPropertiesForKeys: [.contentModificationDateKey])) ?? []
        for e in entries {
            guard let m = try? e.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate,
                  now.timeIntervalSince(m) > age else { continue }
            try? fm.removeItem(at: e)
        }
    }

    private static func removeRoot() { try? FileManager.default.removeItem(at: root) }
}
