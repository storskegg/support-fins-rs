import Foundation
import os
import sf_step_xpc

// The StepReader XPC service (X-01): OpenCascade reads STEP files here, out of the app's process. A malformed file can
// make it dereference null (104 of 325 small mutations of the parity fixtures) or loop forever (5 of them); here that
// ends this process, not the app. Sandboxed with no entitlement beyond the sandbox: the file comes in as bytes and the
// result goes back as bytes.

// a fault ends the process with an exit status and one line on stderr, no crash report (crates/sf-step-xpc)
sf_step_xpc_exit_on_crash()
OpenCascadeLog.capture()

/// OpenCascade's diagnostics (X-13): it prints them (`*** ERR StepReaderData : Unresolved Reference ...`, in colour) on
/// stdout and stderr. Both become a pipe whose lines go to this service's log (subsystem
/// `com.storskegg.SupportFins.StepReader`), so they never reach the app's output.
enum OpenCascadeLog {
    static let logger = Logger(subsystem: StepReaderService.name, category: "OpenCascade")

    static func capture() {
        let pipe = Pipe()
        let write = pipe.fileHandleForWriting.fileDescriptor
        dup2(write, STDOUT_FILENO)
        dup2(write, STDERR_FILENO)
        setvbuf(stdout, nil, _IOLBF, 0)
        let read = pipe.fileHandleForReading
        let thread = Thread {
            var pending = Data()
            while true {
                let chunk = read.availableData
                if chunk.isEmpty { return }
                pending.append(chunk)
                while let nl = pending.firstIndex(of: UInt8(ascii: "\n")) {
                    log(pending[pending.startIndex..<nl])
                    pending.removeSubrange(pending.startIndex...nl)
                }
            }
        }
        thread.name = "OpenCascade log"
        thread.start()
    }

    /// One line, without its ANSI colour codes.
    private static func log(_ line: Data) {
        let text = String(decoding: line, as: UTF8.self).replacing(/\u{1B}\[[0-9;]*m/, with: "")
        guard !text.isEmpty else { return }
        logger.log("\(text, privacy: .public)")
    }
}

/// One connection's reader. A read runs off the connection's queue, so the connection can still be invalidated while
/// OpenCascade works: when the app gives up (its timeout, a cancelled load) or goes away with a read in flight, this
/// process exits, since OpenCascade may be in an endless loop and nothing else stops it.
final class Reader: NSObject, StepReading, @unchecked Sendable {
    private let lock = NSLock()
    private var reading = false

    func read(_ file: Data, reply: @escaping @Sendable (Data) -> Void) {
        lock.withLock { reading = true }
        DispatchQueue.global(qos: .userInitiated).async {
            let bytes = file.withUnsafeBytes { (buf: UnsafeRawBufferPointer) -> Data in
                var n = 0
                guard let p = sf_step_xpc_read(buf.bindMemory(to: UInt8.self).baseAddress, buf.count, &n) else { return Data() }
                return Data(bytesNoCopy: p, count: n, deallocator: .custom { p, n in sf_step_xpc_free(p.assumingMemoryBound(to: UInt8.self), n) })
            }
            self.lock.withLock { self.reading = false }
            reply(bytes)
        }
    }

    func connectionEnded() {
        if lock.withLock({ reading }) {
            OpenCascadeLog.logger.log("the app stopped waiting for a read: exiting")
            // _exit: no atexit handlers or static destructors while OpenCascade runs on another thread
            _exit(0)
        }
    }
}

final class Delegate: NSObject, NSXPCListenerDelegate {
    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
        let reader = Reader()
        connection.exportedInterface = NSXPCInterface(with: StepReading.self)
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
