import simd

/// How the Suggest and Strength models read and turn the part's pose. The pose is the engine's `Rot3`: nine
/// numbers, column-major, taking model coordinates to print coordinates. `PartSession` holds the pose and makes
/// a turn one undo step.
@MainActor
struct PoseAccess {
    var rotation: @MainActor () -> [Double]
    var apply: @MainActor ([Double]) -> Void

    static let identity: [Double] = [1, 0, 0, 0, 1, 0, 0, 0, 1]
}

extension PoseAccess {
    /// The window's `PartSession`: the pose it holds, and a turn applied as one undo step (`applySuggestion`
    /// does `histPush()` then turns the part, `ui/suggest.js:20`).
    static func session(_ s: PartSession, name: String = JS.t("Turn Part")) -> PoseAccess {
        PoseAccess(
            rotation: { s.pose.columnMajor },
            apply: { nine in if let pose = Pose(columnMajor: nine) { s.setPose(pose, name: name) } })
    }
}

/// Plain 3x3 column-major helpers on `[Double]`, so the models do their arithmetic on what the engine takes.
enum Rot {
    /// `m * v`
    static func apply(_ m: [Double], _ v: SIMD3<Double>) -> SIMD3<Double> {
        SIMD3(m[0] * v.x + m[3] * v.y + m[6] * v.z, m[1] * v.x + m[4] * v.y + m[7] * v.z, m[2] * v.x + m[5] * v.y + m[8] * v.z)
    }

    /// `m^T * v`: the inverse of a rotation applied to `v`.
    static func applyInverse(_ m: [Double], _ v: SIMD3<Double>) -> SIMD3<Double> {
        SIMD3(m[0] * v.x + m[1] * v.y + m[2] * v.z, m[3] * v.x + m[4] * v.y + m[5] * v.z, m[6] * v.x + m[7] * v.y + m[8] * v.z)
    }
}

import SupportFinsCore

extension FfiError {
    /// The engine's message (JS-identical where JS has one), usable from the engine's callback thread.
    nonisolated var engineMessage: String {
        switch self {
        case .Io(let message), .Format(let message), .Engine(let message): message
        case .LimitExceeded(_, _, _, let message): message
        case .Cancelled: "Cancelled."
        }
    }
}
