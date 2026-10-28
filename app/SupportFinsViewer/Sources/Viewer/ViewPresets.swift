import Foundation
import simd

/// The camera views of `web/ui/viewpresets.js`: Front looks along +Y (the camera on -Y), and a view keeps
/// the target and the distance. Top and Bottom lean a hair off the vertical so they keep a heading.
public enum CameraView: String, CaseIterable, Sendable {
    case front, back, left, right, top, bottom

    static let tilt: Float = 1e-4

    /// The button's tooltip (`VIEWS[key].hint`).
    public var hint: String {
        switch self {
        case .front: "Look at the part from the front."
        case .back: "Look at the part from the back."
        case .left: "Look at the part from the left."
        case .right: "Look at the part from the right."
        case .top: "Look down on the part from above."
        case .bottom: "Look up at the part from below the plate."
        }
    }

    public var label: String { rawValue.prefix(1).uppercased() + rawValue.dropFirst() }

    /// Direction from the target to the camera (`VIEWS[key].dir`).
    public var direction: SIMD3<Float> {
        switch self {
        case .front: SIMD3(0, -1, 0)
        case .back: SIMD3(0, 1, 0)
        case .left: SIMD3(-1, 0, 0)
        case .right: SIMD3(1, 0, 0)
        case .top: SIMD3(0, -Self.tilt, 1)
        case .bottom: SIMD3(0, -Self.tilt, -1)
        }
    }
}

extension OrbitCamera {
    /// Where the camera goes looking back along `dir` at `target`, `dist` from it (`dirPosition`): straight
    /// up or down leans `tilt` toward the front, as Top and Bottom do.
    public static func position(alongFromTarget dir: SIMD3<Float>, target: SIMD3<Float>, distance: Float) -> SIMD3<Float> {
        var d = dir
        if abs(d.x) < 1e-9 && abs(d.y) < 1e-9 { d.y = -CameraView.tilt * abs(d.z) }
        return target + simd_normalize(d) * distance
    }

    /// Look at the part from one of the six sides, keeping the target and the zoom.
    public mutating func apply(_ view: CameraView) {
        var d = view.direction
        if abs(d.x) < 1e-9 && abs(d.y) < 1e-9 { d.y = -CameraView.tilt * abs(d.z) }
        look(alongFromTarget: d)
    }
}
