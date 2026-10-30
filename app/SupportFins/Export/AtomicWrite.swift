import Foundation

/// The one way the app puts a file at a user-chosen URL (audit X-09): the finished bytes are written to a temp file in
/// a directory the app owns, then moved over the target, so a failed or cancelled write leaves an older file of that
/// name as it was.
///
/// The temp file is `.name.sfexport-<uuid>.ext` (the engine picks the format from the extension) in the temporary-items
/// directory for the target's volume (the move stays one rename), else the app's tmp: the sandbox gives a save panel's
/// URL write access to that one file, not to its folder, so a sibling temp is denied there. The finished file is moved
/// over `url` with `replaceItemAt`; if even that is refused, its bytes are written into `url`.
enum AtomicWrite {
    /// Run `work` (given the temp path to write) and put its file at `url`. Returns false when `token` was cancelled
    /// (nothing written). Throws `work`'s error or the move's; `url` is untouched then.
    nonisolated static func write(to url: URL, token: CancelToken = CancelToken(), work: (String) throws -> Void) throws -> Bool {
        let fm = FileManager.default
        let home = (try? fm.url(for: .itemReplacementDirectory, in: .userDomainMask, appropriateFor: url, create: true))
            ?? fm.temporaryDirectory
        let temp = home.appending(path: ".\(url.deletingPathExtension().lastPathComponent).sfexport-\(UUID().uuidString).\(url.pathExtension)")
        defer { try? fm.removeItem(at: temp) }
        try work(temp.path(percentEncoded: false))
        if token.isCancelled { return false }
        do {
            if fm.fileExists(atPath: url.path(percentEncoded: false)) {
                _ = try fm.replaceItemAt(url, withItemAt: temp)
            } else {
                try fm.moveItem(at: temp, to: url)
            }
        } catch {
            // the move needs rights on the folder; the file itself may be all the sandbox allows
            guard !token.isCancelled else { return false }
            try Data(contentsOf: temp).write(to: url)
        }
        return true
    }

    /// `data` at `url`, atomically.
    nonisolated static func write(_ data: Data, to url: URL) throws {
        _ = try write(to: url) { path in try data.write(to: URL(filePath: path)) }
    }

    /// The file at `source`, copied to `url` atomically (a bundled calibration print).
    nonisolated static func copy(from source: URL, to url: URL) throws {
        _ = try write(to: url) { path in try FileManager.default.copyItem(at: source, to: URL(filePath: path)) }
    }
}
