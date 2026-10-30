import Foundation
import Observation
import os

/// Raised by whoever wants a running export to stop; the worker checks it before it moves the finished file into place.
final class CancelToken: Sendable {
    private let flag = OSAllocatedUnfairLock(initialState: false)
    var isCancelled: Bool { flag.withLock { $0 } }
    func cancel() { flag.withLock { $0 = true } }
}

/// One export running off the main thread (docs/audit-2026-10/concurrency.md C-03). The engine writes to a temp file
/// beside the target, which is renamed over it only if the export finished and was not cancelled: a failed or cancelled
/// export leaves no half-written file (and keeps an older file of that name as it was).
///
/// Cancel returns at once; the engine call itself has no cancel point, so it runs to its end in the background and
/// its temp file is deleted then.
@MainActor
@Observable
final class ExportJob {
    /// What is being written ("3MF"), nil when idle: the progress overlay shows while it is set.
    private(set) var title: String?
    var isRunning: Bool { title != nil }

    @ObservationIgnored private let current = OSAllocatedUnfairLock<CancelToken?>(initialState: nil)
    @ObservationIgnored private var serial = 0
    @ObservationIgnored private var waiter: CheckedContinuation<Result<Bool, any Error>, Never>?

    /// Stop what is running; safe from anywhere (a window's deinit): the file is not written. The waiting `run` returns
    /// when the worker ends, or at once through `cancel()`.
    nonisolated func cancelWithoutWaiting() { current.withLock { $0?.cancel() } }

    func cancel() {
        cancelWithoutWaiting()
        if let w = waiter { waiter = nil; w.resume(returning: .success(false)) }
    }

    /// Run `work` (the engine's export, given the temp path to write) off the main thread and put the file at `url`.
    /// Returns true when it was written, false when cancelled; throws the engine's error. One at a time.
    func run(title: String, to url: URL, work: @escaping @Sendable (String) throws -> Void) async throws -> Bool {
        precondition(!isRunning)   // one export at a time
        let token = CancelToken()
        serial += 1
        let mine = serial
        current.withLock { $0 = token }
        self.title = title
        let worker = Task.detached(priority: .userInitiated) { () -> Result<Bool, any Error> in
            Result { try Self.writeAtomically(to: url, token: token, work: work) }
        }
        let outcome = await withCheckedContinuation { (cont: CheckedContinuation<Result<Bool, any Error>, Never>) in
            waiter = cont
            Task { [self] in finish(await worker.value, run: mine) }
        }
        // a later run may already have started after a Cancel: only the one that set the state clears it
        if current.withLock({ $0 === token }) { current.withLock { $0 = nil }; self.title = nil }
        return try outcome.get()
    }

    /// A cancelled run's worker ends later, maybe after the next run began: it must not answer that one.
    private func finish(_ r: Result<Bool, any Error>, run: Int) {
        if run == serial, let w = waiter { waiter = nil; w.resume(returning: r) }
    }

    /// The temp file is `.name.sfexport-<uuid>.ext` (the engine picks the format from the extension) in a directory the
    /// app owns: the sandbox gives a save panel's URL write access to that one file, not to its folder, so a sibling temp
    /// is denied there. The temporary-items directory for `url`'s volume is tried first (the move stays one rename), then
    /// the app's tmp. The finished file is moved over `url` with `replaceItemAt`; if even that is refused, its bytes are
    /// written into `url`. A failed or cancelled export never touches `url`.
    nonisolated static func writeAtomically(to url: URL, token: CancelToken, work: (String) throws -> Void) throws -> Bool {
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
            try fm.copyItem(at: temp, to: url, replacing: true)
        }
        return true
    }
}

private extension FileManager {
    /// Copy over `to`, replacing it: the bytes go into the file in place when it exists (rights on the file only).
    func copyItem(at from: URL, to: URL, replacing: Bool) throws {
        let data = try Data(contentsOf: from)
        try data.write(to: to)
    }
}
