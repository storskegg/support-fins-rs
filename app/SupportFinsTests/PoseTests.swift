import Foundation
import Testing
import simd
@testable import Support_Fins

@Suite("Pose")
struct PoseTests {
    /// (label, rotation column-major, expected readout), computed by running `readableEuler`
    /// (web/ui/part.js:430) in three.js on the quaternion of each pose: every sequence of up to three world-axis
    /// 90 degree turns, and a table of arbitrary XYZ Euler poses.
    static let table: [(String, [Double], [Int])] = [
        ("identity", [1, 0, 0, 0, 1, 0, 0, 0, 1], [0, 0, 0]),
        ("x", [1, 0, 0, 0, 0, 1, 0, -1, 0], [90, 0, 0]),
        ("xx", [1, 0, 0, 0, -1, 0, 0, 0, -1], [180, 0, 0]),
        ("xxx", [1, 0, 0, 0, 0, -1, 0, 1, 0], [-90, 0, 0]),
        ("xxy", [0, 0, -1, 0, -1, 0, -1, 0, 0], [180, -90, 0]),
        ("xxz", [0, 1, 0, 1, 0, 0, 0, 0, -1], [180, 0, -90]),
        ("xy", [0, 0, -1, 1, 0, 0, 0, -1, 0], [90, 0, -90]),
        ("xyy", [-1, 0, 0, 0, 0, -1, 0, -1, 0], [90, 0, 180]),
        ("xyz", [0, 0, -1, 0, 1, 0, 1, 0, 0], [0, 90, 0]),
        ("xz", [0, 1, 0, 0, 0, 1, 1, 0, 0], [90, 90, 0]),
        ("xzx", [0, 0, 1, 0, -1, 0, 1, 0, 0], [180, 90, 0]),
        ("xzz", [-1, 0, 0, 0, 0, 1, 0, 1, 0], [-90, 0, 180]),
        ("yy", [-1, 0, 0, 0, 1, 0, 0, 0, -1], [0, 180, 0]),
        ("yyy", [0, 0, 1, 0, 1, 0, -1, 0, 0], [0, -90, 0]),
        ("yyz", [0, -1, 0, -1, 0, 0, 0, 0, -1], [180, 0, 90]),
        ("yz", [0, 0, -1, -1, 0, 0, 0, 1, 0], [-90, 0, 90]),
        ("yzx", [0, 1, 0, -1, 0, 0, 0, 0, 1], [0, 0, 90]),
        ("zx", [0, 0, 1, -1, 0, 0, 0, -1, 0], [90, 0, 90]),
        ("zz", [-1, 0, 0, 0, -1, 0, 0, 0, 1], [0, 0, 180]),
        ("zzz", [0, -1, 0, 1, 0, 0, 0, 0, 1], [0, 0, -90]),
        ("euler 30 0 0", [1, 0, 0, 0, 0.866025403784, 0.5, 0, -0.5, 0.866025403784], [30, 0, 0]),
        ("euler 0 40 0", [0.766044443119, 0, -0.642787609687, 0, 1, 0, 0.642787609687, 0, 0.766044443119], [0, 40, 0]),
        ("euler 0 0 50", [0.642787609687, 0.766044443119, 0, -0.766044443119, 0.642787609687, 0, 0, 0, 1], [0, 0, 50]),
        ("euler 30 40 50", [0.492403876506, 0.870001903752, 0.025201386257, -0.586824088833, 0.310468460973, 0.747828070819, 0.642787609687, -0.383022221559, 0.663413948169], [30, 40, 50]),
        ("euler -120 10 75", [0.254887002244, -0.521885087365, -0.814044575974, -0.951251242564, 0.015850009181, -0.30800949779, 0.173648177667, 0.852868531952, -0.492403876506], [-120, 10, 75]),
        ("euler 10 89 20", [0.016399897544, 0.499975147518, -0.865884458357, -0.005969074551, 0.866034449348, 0.49994870006, 0.999847695156, -0.003030578574, 0.017187265168], [10, 89, 20]),
        ("euler 45 45 45", [0.5, 0.853553390593, 0.146446609407, -0.5, 0.146446609407, 0.853553390593, 0.707106781187, -0.5, 0.5], [45, 45, 45]),
        ("euler 179 0 0", [1, 0, 0, 0, -0.999847695156, 0.017452406437, 0, -0.017452406437, -0.999847695156], [179, 0, 0]),
        ("euler 0 0 179", [-0.999847695156, 0.017452406437, 0, -0.017452406437, -0.999847695156, 0, 0, 0, 1], [0, 0, 179]),
        ("euler 200 -30 10", [0.852868531952, 0.00523613325, -0.522099463813, -0.15038373318, -0.955112165705, -0.25523613325, -0.5, 0.296198132726, -0.813797681349], [-160, -30, 10]),
        ("euler 12 -90 33", [0, 0.358367949545, 0.933580426497, 0, 0.933580426497, -0.358367949545, -1, 0, 0], [-21, -90, 0]),
    ]

    @Test(arguments: table)
    func readoutMatchesTheWebFormula(row: (String, [Double], [Int])) throws {
        let pose = try #require(Pose(columnMajor: row.1))
        let e = pose.euler
        #expect([e.x, e.y, e.z] == row.2, "\(row.0): \(e.readout)")
    }

    @Test func readoutStringIsTheWebText() {
        #expect(Pose.identity.euler.readout == "X 0° · Y 0° · Z 0°")
        #expect(Pose.identity.rotated90(about: .x).euler.readout == "X 90° · Y 0° · Z 0°")
        // two quarter turns about Y read as "Y 180", not "X -180, Y 0, Z -180" (the alternate solution)
        #expect(Pose.identity.rotated90(about: .y).rotated90(about: .y).euler == Pose.Euler(0, 180, 0))
    }

    @Test func rotatingAboutAWorldAxisPremultiplies() {
        // X then Z (world axes) is not Z then X
        let a = Pose.identity.rotated90(about: .x).rotated90(about: .z)
        let b = Pose.identity.rotated90(about: .z).rotated90(about: .x)
        #expect(!a.isSame(as: b))
        // a quarter turn about world Z takes +X to +Y whatever came before
        let v = a.rotation * SIMD3<Double>(1, 0, 0)
        #expect(simd_length(v - SIMD3(0, 1, 0)) < 1e-12)
        // four quarter turns are the identity
        var p = Pose.identity
        for _ in 0..<4 { p = p.rotated90(about: .y) }
        #expect(p.isSame(as: .identity))
    }

    @Test func columnMajorRoundTripsAndRejectsBadInput() throws {
        let p = Pose.identity.rotated(about: .x, degrees: 30).rotated(about: .y, degrees: 40)
        let back = try #require(Pose(columnMajor: p.columnMajor))
        #expect(back == p)
        #expect(Pose(columnMajor: [1, 0, 0]) == nil)
        #expect(Pose(columnMajor: [Double](repeating: .nan, count: 9)) == nil)
        // the engine reads columns: the x axis image is the first three numbers
        let q = Pose.identity.rotated90(about: .z)
        #expect(q.columnMajor.prefix(3).map { $0.rounded() } == [0, 1, 0])
    }

    @Test func roundingGoesUpOnHalvesLikeMathRound() {
        #expect(Pose.jsRound(0.5) == 1)
        #expect(Pose.jsRound(-0.5) == 0)
        #expect(Pose.jsRound(-1.5) == -1)
        #expect(Pose.jsRound(29.6) == 30)
    }

    @Test func wrapKeepsHalfATurnPositiveAndNeverNegativeZero() {
        #expect(Pose.wrap180(-180) == 180)
        #expect(Pose.wrap180(180) == 180)
        #expect(Pose.wrap180(540) == 180)
        #expect(Pose.wrap180(-360) == 0 && Pose.wrap180(-360).sign == .plus)
        #expect(Pose.wrap180(190) == -170)
    }
}
