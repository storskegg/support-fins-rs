import Foundation

/// What `FfiError.errorDescription` says (hand-written; `just swift-bindings` points the generated getter here instead
/// of UniFFI's `String(reflecting: self)`, which showed users `SupportFinsCore.FfiError.Format(message: "...")`).
/// The engine's own message, the JS-identical one where the JS has one.
func ffiErrorDescription(_ error: FfiError) -> String {
    switch error {
    case .Io(let message), .Format(let message), .Engine(let message): message
    case .LimitExceeded(_, _, _, let message): message
    case .Cancelled: "Cancelled."
    }
}
