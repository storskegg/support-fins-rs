import Foundation

/// What the app asks its StepReader XPC service (`app/StepReader`, X-01): read a STEP file's bytes with OpenCascade.
/// OpenCascade crashes on some malformed files, so it runs there, never in the app; a crash ends the service's process,
/// and the app sees the connection drop. Compiled into both the app and the service.
@objc protocol StepReading {
    /// `reply` gets `sf_step::wire` bytes: the kernel's result, its `success: false`, or the reader's own error. The app
    /// decodes them as untrusted (`load_step_parts`).
    func read(_ file: Data, reply: @escaping @Sendable (Data) -> Void)
}

enum StepReaderService {
    /// The service's bundle identifier (app/project.yml), the name an `NSXPCConnection` looks it up by.
    static let name = "com.storskegg.SupportFins.StepReader"
}
