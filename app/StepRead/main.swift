import Foundation
import os
import sf_step_xpc

// step-read (X-01): one STEP read with OpenCascade, in a process of its own. The StepReader XPC service starts one per
// file (`app/StepReader/main.swift`) with two paths in its own temporary directory: the file's bytes to read, and where to
// write the `sf_step::wire` bytes; then exit 0. A malformed file can make OpenCascade dereference null (101 of the 106 files
// in crates/sf-step-xpc/tests/segv) or loop forever (5): a fault ends this process with exit status 128 + the signal and no
// crash report (`sf_step_xpc_exit_on_crash`); a loop ends when the service kills it. Sandboxed, inheriting the service's
// sandbox (which reaches no file but its own container).

sf_step_xpc_exit_on_crash()
// OpenCascade's own printing (stdout and stderr) goes to the log (X-13)
let capture = OpenCascadeLog.capture()

let args = CommandLine.arguments
guard args.count == 3, let file = FileManager.default.contents(atPath: args[1]) else {
    OpenCascadeLog.logger.error("step-read: usage: step-read <file> <result>")
    _exit(64)
}
var length = 0
let bytes = file.withUnsafeBytes { (buf: UnsafeRawBufferPointer) in
    sf_step_xpc_read(buf.bindMemory(to: UInt8.self).baseAddress, buf.count, &length)
}
let written = bytes.map { FileManager.default.createFile(atPath: args[2], contents: Data(bytesNoCopy: $0, count: length, deallocator: .none)) } ?? false
OpenCascadeLog.logger.debug("step-read: \(file.count) bytes in, \(length) bytes out")
capture.finish()
// _exit: no static destructors (OpenCascade's) to run on the way out
_exit(written ? 0 : 74)

/// OpenCascade's diagnostics (X-13): it prints them (`*** ERR StepReaderData : Unresolved Reference ...`, in colour) on
/// stdout and stderr. Both become a pipe whose lines go to the log (subsystem `com.storskegg.SupportFins.StepReader`,
/// category `OpenCascade`), so they never reach the app's output, nor the result.
enum OpenCascadeLog {
    static let logger = Logger(subsystem: "com.storskegg.SupportFins.StepReader", category: "OpenCascade")

    /// The capture: `finish()` closes the pipe and waits (a second at most) for the last lines to be logged.
    struct Capture {
        let pipe: Pipe
        let done: DispatchSemaphore

        func finish() {
            fflush(stdout)
            fflush(stderr)
            let null = open("/dev/null", O_WRONLY)
            dup2(null, STDOUT_FILENO)
            dup2(null, STDERR_FILENO)
            try? pipe.fileHandleForWriting.close()
            _ = done.wait(timeout: .now() + 1)
        }
    }

    static func capture() -> Capture {
        let pipe = Pipe()
        let done = DispatchSemaphore(value: 0)
        let write = pipe.fileHandleForWriting.fileDescriptor
        dup2(write, STDOUT_FILENO)
        dup2(write, STDERR_FILENO)
        setvbuf(stdout, nil, _IOLBF, 0)
        let read = pipe.fileHandleForReading
        let thread = Thread {
            var pending = Data()
            while true {
                let chunk = read.availableData
                if chunk.isEmpty { break }
                pending.append(chunk)
                while let nl = pending.firstIndex(of: UInt8(ascii: "\n")) {
                    log(pending[pending.startIndex..<nl])
                    pending.removeSubrange(pending.startIndex...nl)
                }
            }
            log(pending)
            done.signal()
        }
        thread.name = "OpenCascade log"
        thread.start()
        return Capture(pipe: pipe, done: done)
    }

    /// One line, without its ANSI colour codes.
    private static func log(_ line: Data) {
        let text = String(decoding: line, as: UTF8.self).replacing(/\u{1B}\[[0-9;]*m/, with: "")
        guard !text.isEmpty else { return }
        logger.log("\(text, privacy: .public)")
    }
}
