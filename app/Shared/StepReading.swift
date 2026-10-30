import Foundation

/// What the app asks its StepReader XPC service (`app/StepReader`, X-01): read a STEP file's bytes with OpenCascade.
/// OpenCascade crashes on some malformed files, so it never runs in the app: the service runs it in a `step-read` process
/// per file (`app/StepRead`). Compiled into both the app and the service.
@objc protocol StepReading {
    func read(_ file: Data, reply: @escaping @Sendable (StepReadReply) -> Void)
}

/// How a read ended: `step-read`'s exit status (128 + the signal when OpenCascade faulted, -1 when it could not start)
/// and, when it exited 0, its `sf_step::wire` bytes (the kernel's result, its `success: false`, or the reader's own
/// error), which the app decodes as untrusted (`load_step_parts`).
@objc(StepReadReply) final class StepReadReply: NSObject, NSSecureCoding, Sendable {
    static let supportsSecureCoding = true
    let bytes: Data?
    let status: Int32

    init(bytes: Data?, status: Int32) {
        self.bytes = bytes
        self.status = status
    }

    func encode(with coder: NSCoder) {
        coder.encode(bytes, forKey: "bytes")
        coder.encode(status, forKey: "status")
    }

    init?(coder: NSCoder) {
        bytes = coder.decodeObject(of: NSData.self, forKey: "bytes") as Data?
        status = coder.decodeInt32(forKey: "status")
    }
}

enum StepReaderService {
    /// The service's bundle identifier (app/project.yml), the name an `NSXPCConnection` looks it up by.
    static let name = "com.storskegg.SupportFins.StepReader"

    /// The reply's class is in the method's signature, so XPC decodes it without a class list.
    static func interface() -> NSXPCInterface { NSXPCInterface(with: StepReading.self) }
}
