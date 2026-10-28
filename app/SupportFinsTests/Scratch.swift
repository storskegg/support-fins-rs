import Foundation

/// Scratch files for tests. Swift Testing runs tests in parallel, so a fixed path under the temp directory is
/// shared state: one test rewrites a file while another reads it. Every call gets its own directory.
enum Scratch {
    /// A new, empty directory under the temp directory; remove it when the test is done (`remove`), or leave it
    /// to the system's temp clean-up.
    static func directory() throws -> URL {
        let dir = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString, directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// `name` in a directory of its own: a fixed file name (the part's name shows in the export's file name) at
    /// a path nobody else writes.
    static func file(_ name: String) throws -> URL {
        try directory().appending(path: name)
    }

    static func remove(_ url: URL) { try? FileManager.default.removeItem(at: url) }
}
