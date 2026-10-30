import Foundation
import os

// The StepReader XPC service (X-01): the app's STEP files are read here, out of its process. OpenCascade itself runs one
// level further down, in a `step-read` process per file (Contents/MacOS/step-read, `app/StepRead`), which inherits this
// service's sandbox (no file access: the bytes come in over XPC). A malformed file can make OpenCascade fault or loop
// forever; that ends or hangs only its `step-read`. This service stays up: a service that exits soon after launch is
// relaunched by launchd only after a 10 s penalty, which the next file would wait out.

let logger = Logger(subsystem: StepReaderService.name, category: "service")

/// One connection's reads. Each read is a `step-read` process, given the file and a place for its result in this
/// service's temporary directory; when the connection ends (the app's timeout, a cancelled load, the app quitting) its
/// reads still running are killed.
final class Reader: NSObject, StepReading, @unchecked Sendable {
    private let lock = NSLock()
    private var running: [Process] = []
    private var ended = false

    func read(_ file: Data, reply: @escaping @Sendable (StepReadReply) -> Void) {
        let dir = FileManager.default.temporaryDirectory.appending(path: "step-read-\(UUID().uuidString)", directoryHint: .isDirectory)
        let input = dir.appending(path: "file.step"), output = dir.appending(path: "result")
        let fail = { (why: String) in
            logger.error("step-read did not start: \(why, privacy: .public)")
            try? FileManager.default.removeItem(at: dir)
            reply(StepReadReply(bytes: nil, status: -1))
        }
        guard let exe = Bundle.main.url(forAuxiliaryExecutable: "step-read") else { return fail("no step-read in the bundle") }
        do {
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            try file.write(to: input)
        } catch {
            return fail(error.localizedDescription)
        }
        let process = Process()
        process.executableURL = exe
        process.arguments = [input.path(percentEncoded: false), output.path(percentEncoded: false)]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        process.terminationHandler = { [self] p in
            forget(p)
            let status = p.terminationReason == .exit ? p.terminationStatus : 128 + p.terminationStatus
            let bytes = status == 0 ? try? Data(contentsOf: output) : nil
            try? FileManager.default.removeItem(at: dir)
            logger.debug("step-read \(p.processIdentifier): status \(status), \(bytes?.count ?? 0) bytes")
            reply(StepReadReply(bytes: bytes, status: bytes == nil && status == 0 ? -1 : status))
        }
        let started: Bool = lock.withLock {
            guard !ended else { return false }
            running.append(process)
            return true
        }
        guard started else { try? FileManager.default.removeItem(at: dir); return }
        do {
            try process.run()
        } catch {
            forget(process)
            return fail(error.localizedDescription)
        }
    }

    private func forget(_ process: Process) {
        lock.withLock { running.removeAll { $0 === process } }
    }

    /// The app stopped waiting: kill what is still reading (OpenCascade may be in an endless loop).
    func connectionEnded() {
        let left = lock.withLock {
            ended = true
            return running
        }
        for p in left where p.isRunning {
            logger.log("the app stopped waiting for a read: killing step-read \(p.processIdentifier)")
            kill(p.processIdentifier, SIGKILL)
        }
    }
}

final class Delegate: NSObject, NSXPCListenerDelegate {
    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
        let reader = Reader()
        connection.exportedInterface = StepReaderService.interface()
        connection.exportedObject = reader
        connection.invalidationHandler = { reader.connectionEnded() }
        connection.resume()
        return true
    }
}

let delegate = Delegate()
let listener = NSXPCListener.service()
listener.delegate = delegate
listener.resume()
