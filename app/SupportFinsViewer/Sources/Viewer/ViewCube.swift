import Foundation
import simd

/// The view cube's geometry and hit-testing (`web/ui/viewcube.js`, `CUBE_FACES` and `cubeCellDir` in
/// `web/ui/viewpresets.js`). Orthographic like the web's CSS 3D: a face is a square of edge `size`, centred
/// `size / 2` out along its normal, with a 3x3 grid of click targets: the middle one is the face, an edge strip
/// the view halfway to the face beyond it, a corner the view from that corner of the cube.
public enum ViewCube {
    public static let size: Float = 64                  // S, px
    public static let edge: Float = 13                  // the grid's 13px / 1fr / 13px
    public static let stage: Float = 116                // .vc-stage, px

    public enum Face: String, CaseIterable, Sendable {
        case front, back, left, right, top, bottom

        public var label: String { rawValue.prefix(1).uppercased() + rawValue.dropFirst() }

        /// The face's name as the cube draws it: `t(VIEWS[key].label)` (`ui/viewcube.js:59`), the web's translation from
        /// the app's String Catalog (`bundle` is the app's; a test passes one language's `.lproj`).
        public func localizedLabel(bundle: Bundle = .main, locale: Locale = .current) -> String {
            String(localized: String.LocalizationValue(label), bundle: bundle, locale: locale)
        }

        /// `n` points out of the face, `u` is right and `v` down on it read from outside, so its label stands
        /// upright in that face's own view.
        public var n: SIMD3<Float> {
            switch self {
            case .front: [0, -1, 0]
            case .back: [0, 1, 0]
            case .left: [-1, 0, 0]
            case .right: [1, 0, 0]
            case .top: [0, 0, 1]
            case .bottom: [0, 0, -1]
            }
        }
        public var u: SIMD3<Float> {
            switch self {
            case .front, .top, .bottom: [1, 0, 0]
            case .back: [-1, 0, 0]
            case .left: [0, -1, 0]
            case .right: [0, 1, 0]
            }
        }
        public var v: SIMD3<Float> {
            switch self {
            case .front, .back, .left, .right: [0, 0, -1]
            case .top: [0, -1, 0]
            case .bottom: [0, 1, 0]
            }
        }
    }

    /// The view direction for cell (`col`, `row`), each -1, 0 or 1, of a face (`cubeCellDir`).
    public static func cellDirection(_ face: Face, col: Int, row: Int) -> SIMD3<Int> {
        let n = face.n, u = face.u, v = face.v
        let f = SIMD3<Float>(n + Float(col) * u + Float(row) * v)
        return SIMD3(Int(f.x.rounded()), Int(f.y.rounded()), Int(f.z.rounded()))
    }

    public struct Cell: Equatable, Sendable {
        public var face: Face
        public var col: Int
        public var row: Int
        public var direction: SIMD3<Int> { ViewCube.cellDirection(face, col: col, row: row) }
    }

    /// The camera's axes as the cube sees them (`syncViewCube`): world vectors of screen right, screen up and
    /// back toward the viewer.
    public struct Basis: Sendable {
        public var right: SIMD3<Float>
        public var up: SIMD3<Float>
        public var back: SIMD3<Float>

        public init(camera: OrbitCamera) {
            let b = camera.basis
            right = b.right; up = b.up; back = -b.forward
        }

        /// A world vector on the screen, x right and y DOWN, relative to the cube's centre.
        public func project(_ p: SIMD3<Float>) -> SIMD2<Float> { SIMD2(simd_dot(p, right), -simd_dot(p, up)) }
    }

    /// A face turned toward the viewer (`backface-visibility: hidden`).
    public static func isVisible(_ face: Face, _ basis: Basis) -> Bool { simd_dot(face.n, basis.back) > 1e-6 }

    /// A face's frame on the screen: its centre, and where one step along its `u` and `v` goes.
    public static func frame(_ face: Face, _ basis: Basis) -> (origin: SIMD2<Float>, u: SIMD2<Float>, v: SIMD2<Float>) {
        (basis.project(face.n * (size / 2)), basis.project(face.u), basis.project(face.v))
    }

    /// The cell under `point`, in points from the cube's centre (y down), if any.
    public static func hit(_ point: SIMD2<Float>, _ basis: Basis) -> Cell? {
        for face in Face.allCases where isVisible(face, basis) {
            let f = frame(face, basis)
            let det = f.u.x * f.v.y - f.u.y * f.v.x
            guard abs(det) > 1e-6 else { continue }
            let d = point - f.origin
            let a = (d.x * f.v.y - d.y * f.v.x) / det
            let b = (f.u.x * d.y - f.u.y * d.x) / det
            let h = size / 2
            guard abs(a) <= h, abs(b) <= h else { continue }
            func strip(_ x: Float) -> Int { x < -h + edge ? -1 : x > h - edge ? 1 : 0 }
            return Cell(face: face, col: strip(a), row: strip(b))
        }
        return nil
    }
}
