import Foundation

/// Reads STEP files in the app's StepReader XPC service (X-01): OpenCascade crashes or loops forever on some malformed
/// files, and in the app that killed every window. The service runs each read in a `step-read` process of its own and
/// sends back `sf_step::wire` bytes, which `load_step_parts` decodes. A read that crashed `step-read`, a connection that
/// was interrupted or invalidated, and a read that did not end within `timeout` are `Failure`: "Couldn't read this STEP
/// file", after the `Could not read <name>:` prefix (`ReadFailure`). A cancelled task cancels the read. When the app
/// stops waiting (timeout, cancel) the connection is invalidated, and the service kills that read's `step-read`.
enum StepReader {
    /// How long a read may take. The parity fixtures take 40 ms; a big assembly takes seconds; some malformed files make
    /// OpenCascade loop forever.
    static let timeout: Duration = .seconds(120)

    struct Failure: LocalizedError, Equatable {
        enum Reason: Equatable { case crashed, connection, timeout }
        let reason: Reason
        var errorDescription: String? { JS.t("Couldn't read this STEP file") }
    }

    /// The service's read of a STEP file's bytes.
    static func read(_ file: Data, timeout: Duration = timeout) async throws -> Data {
        let call = Call()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                call.start(file, timeout: timeout, continuation: continuation)
            }
        } onCancel: {
            call.finish(.failure(CancellationError()))
        }
    }

    /// One read on a connection of its own: whichever comes first of the reply, the connection dropping, the timeout
    /// and cancellation ends it, once, and invalidates the connection.
    private final class Call: @unchecked Sendable {
        private let lock = NSLock()
        private var continuation: CheckedContinuation<Data, Error>?
        private var connection: NSXPCConnection?
        private var timer: DispatchWorkItem?
        private var outcome: Result<Data, Error>?

        func start(_ file: Data, timeout: Duration, continuation: CheckedContinuation<Data, Error>) {
            let connection = NSXPCConnection(serviceName: StepReaderService.name)
            connection.remoteObjectInterface = StepReaderService.interface()
            connection.interruptionHandler = { [self] in finish(.failure(Failure(reason: .connection))) }
            connection.invalidationHandler = { [self] in finish(.failure(Failure(reason: .connection))) }
            let timer = DispatchWorkItem { [self] in finish(.failure(Failure(reason: .timeout))) }
            let ended: Result<Data, Error>? = lock.withLock {
                // cancelled before it started
                if let outcome { return outcome }
                self.continuation = continuation
                self.connection = connection
                self.timer = timer
                return nil
            }
            if let ended {
                continuation.resume(with: ended)
                return
            }
            connection.resume()
            let proxy = connection.remoteObjectProxyWithErrorHandler { [self] _ in finish(.failure(Failure(reason: .connection))) }
            guard let reader = proxy as? StepReading else {
                finish(.failure(Failure(reason: .connection)))
                return
            }
            let seconds = Double(timeout.components.seconds) + Double(timeout.components.attoseconds) * 1e-18
            DispatchQueue.global().asyncAfter(deadline: .now() + seconds, execute: timer)
            reader.read(file) { [self] reply in
                if reply.status == 0, let bytes = reply.bytes {
                    finish(.success(bytes))
                } else {
                    finish(.failure(Failure(reason: .crashed)))
                }
            }
        }

        func finish(_ result: Result<Data, Error>) {
            let ended: (CheckedContinuation<Data, Error>?, NSXPCConnection?, DispatchWorkItem?)? = lock.withLock {
                guard outcome == nil else { return nil }
                outcome = result
                defer { continuation = nil; connection = nil; timer = nil }
                return (continuation, connection, timer)
            }
            guard let (continuation, connection, timer) = ended else { return }
            timer?.cancel()
            connection?.invalidate()
            continuation?.resume(with: result)
        }
    }
}
