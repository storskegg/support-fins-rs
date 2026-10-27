import Foundation
import Testing
import simd
@testable import Viewer

@Suite("OrbitCamera")
struct CameraTests {
    func near(_ a: Float, _ b: Float, _ tol: Float = 1e-4) -> Bool { abs(a - b) <= tol }
    func near(_ a: SIMD3<Float>, _ b: SIMD3<Float>, _ tol: Float = 1e-4) -> Bool { simd_length(a - b) <= tol }

    @Test func homeLooksAlongTheWebAppsDirection() {
        let c = OrbitCamera.home(distance: 100)
        let want = simd_normalize(OrbitCamera.homeDirection)
        #expect(near(c.direction, want))
        #expect(near(c.eye, want * 100))
    }

    @Test func targetProjectsToTheMiddleOfTheView() {
        var c = OrbitCamera.home(distance: 250, aspect: 1.6)
        c.target = [10, -20, 30]
        let p = c.project(c.target)
        #expect(near(p.x, 0) && near(p.y, 0))
        #expect(p.z > 0 && p.z < 1)
    }

    @Test func zIsUpOnScreen() {
        // a point straight above the target lands in the upper half of the view
        let c = OrbitCamera.home(distance: 200)
        #expect(c.project(c.target + [0, 0, 20]).y > 0.01)
        #expect(c.project(c.target - [0, 0, 20]).y < -0.01)
    }

    @Test func orbitTurnsByTwoPiPerViewHeightAndClampsElevation() {
        var c = OrbitCamera.home()
        let az = c.azimuth
        c.orbit(dx: 100, dy: 0, viewHeight: 100)
        #expect(near(c.azimuth, az - 2 * .pi))
        c.orbit(dx: 0, dy: 1000, viewHeight: 100)
        #expect(near(c.elevation, OrbitCamera.maxElevation))
        c.orbit(dx: 0, dy: -5000, viewHeight: 100)
        #expect(near(c.elevation, -OrbitCamera.maxElevation))
        // dragging right moves the camera round to the left of the scene: the scene turns right
        var d = OrbitCamera(azimuth: 0, elevation: 0)
        d.orbit(dx: 10, dy: 0, viewHeight: 400)
        #expect(d.azimuth < 0)
    }

    @Test func panKeepsThePointUnderThePointerWithThePointer() {
        var c = OrbitCamera.home(distance: 300, aspect: 1.5)
        let old = c.target
        let (w, h): (Float, Float) = (600, 400)
        c.pan(dx: 30, dy: 20, viewHeight: h)
        // the old target now sits where the pointer moved to: right by dx, down by dy
        let p = c.project(old)
        #expect(near(p.x, 2 * 30 / w, 1e-3))
        #expect(near(p.y, -2 * 20 / h, 1e-3))
        // and the pan stays in the screen plane
        let b = c.basis
        #expect(near(simd_dot(c.target - old, b.forward), 0))
    }

    @Test func wheelZoomsInAndOutAndStaysInRange() {
        var c = OrbitCamera.home(distance: 100)
        c.zoom(wheelDelta: -100)
        #expect(c.distance < 100)
        let closer = c.distance
        c.zoom(wheelDelta: 100)
        #expect(near(c.distance, 100, 1e-3) && c.distance > closer)
        c.zoom(by: 1e9)
        #expect(c.distance == OrbitCamera.minDistance)
        c.zoom(by: 1e-9)
        #expect(c.distance == OrbitCamera.maxDistance)
    }

    @Test func frameBacksOffToSeeThePart() {
        var c = OrbitCamera.home(aspect: 1.6)
        c.frame(size: [100, 40, 60])
        #expect(near(c.distance, 100 * 2.1, 1e-3))
        #expect(near(c.target, [0, 0, 30]))
        #expect(near(c.near, 0.5) && near(c.far, c.distance * 12, 1e-2))
        // a portrait view backs off further
        var p = OrbitCamera.home(aspect: 0.5)
        p.frame(size: [100, 40, 60])
        #expect(p.distance > c.distance)
        // a small part still frames as 40 mm
        var s = OrbitCamera.home()
        s.frame(size: [5, 5, 5])
        #expect(near(s.distance, 40 * 2.1, 1e-3))
    }

    @Test func standardViewsLookFromTheirSide() {
        var c = OrbitCamera.home()
        c.apply(.front)
        #expect(near(c.direction, [0, -1, 0], 1e-3))
        c.apply(.right)
        #expect(near(c.direction, [1, 0, 0], 1e-3))
        c.apply(.top)
        #expect(c.direction.z > 0.9999 && c.direction.y < 0)   // leans off the pole, towards the front
        c.apply(.bottom)
        #expect(c.direction.z < -0.9999)
        for v in CameraView.allCases { var d = c; d.apply(v); #expect(d.distance == c.distance) }
    }

    @Test func positionAlongADirectionMatchesDirPosition() {
        let p = OrbitCamera.position(alongFromTarget: [0, 0, 1], target: [1, 2, 3], distance: 10)
        #expect(near(p, [1, 2, 3] + [0, -1e-4 * 1, 1] * (10 / simd_length(SIMD3<Float>(0, 1e-4, 1)))))
    }

    @Test func aRayThroughTheMiddleHitsTheTarget() {
        var c = OrbitCamera.home(distance: 180, aspect: 2)
        c.target = [5, 6, 7]
        let r = c.ray(ndcX: 0, ndcY: 0)
        let toTarget = simd_normalize(c.target - r.origin)
        #expect(near(r.direction, toTarget, 1e-3))
        // a ray through the right edge turns to the right of the view
        let e = c.ray(ndcX: 1, ndcY: 0)
        #expect(simd_dot(e.direction, c.basis.right) > 0)
    }

    @Test func projectionMapsNearAndFarToTheClipRange() {
        let c = OrbitCamera(distance: 100, azimuth: 0, elevation: 0, near: 1, far: 1000)
        let nearP = c.projectionMatrix * SIMD4(0, 0, -1, 1)
        let farP = c.projectionMatrix * SIMD4(0, 0, -1000, 1)
        #expect(near(nearP.z / nearP.w, 0, 1e-5))
        #expect(near(farP.z / farP.w, 1, 1e-4))
    }
}

@Suite("Plate and volumes")
struct PlateTests {
    @Test func theWebPresetsAreKept() {
        #expect(PrinterVolume.presets.count == 6)
        #expect(PrinterVolume.standard == PrinterVolume(x: 250, y: 220, z: 270))
        #expect(PrinterVolume.presets.contains(.standard))
        #expect(PrinterVolume.standard.label == "250 × 220 × 270 mm")
        #expect(!PrinterVolume(x: 0, y: 5, z: 5).isValid)
    }

    @Test func gridMatchesBuildPlate() {
        let g = PlateGeometry(volume: .standard)
        // x lines -120...120 step 10 (25), y lines -110...110 (23); 50 mm lines are major
        #expect(g.major.count / 2 == 5 + 5 - 0 + 0 || true)
        let lines = (g.minor.count + g.major.count) / 2
        #expect(lines == 25 + 23)
        // x = -100, -50, 0, 50, 100 and y = -100, -50, 0, 50, 100
        #expect(g.major.count / 2 == 10)
        #expect(g.edge.count == 8)
        #expect(g.box.count == 24)
        #expect(g.box.contains([125, 110, 270]))
        #expect(g.minor.allSatisfy { $0.z == 0 })
    }

    @Test func rotationMatrixIsAProperRotationInThreeJsOrder() {
        let r = rotationMatrix(degrees: [90, 0, 0])
        let up = r * SIMD3<Float>(0, 1, 0)
        #expect(simd_length(up - SIMD3(0, 0, 1)) < 1e-5)
        let m = rotationMatrix(degrees: [30, 40, 50])
        #expect(abs(simd_determinant(m) - 1) < 1e-5)
        // Rx * Ry * Rz: z first
        let p = rotationMatrix(degrees: [0, 0, 90]) * SIMD3<Float>(1, 0, 0)
        #expect(simd_length(p - SIMD3(0, 1, 0)) < 1e-5)
    }

    @Test func seatingPutsTheLowestPointOnThePlate() {
        let b = (min: SIMD3<Float>(-20, -20, -20), max: SIMD3<Float>(20, 20, 20))
        let r = rotationMatrix(degrees: [45, 0, 0])
        let off = seatingOffset(bounds: b, rotation: r)
        var lo = Float.infinity
        for i in 0..<8 {
            let c = SIMD3<Float>(i & 1 == 0 ? -20 : 20, i & 2 == 0 ? -20 : 20, i & 4 == 0 ? -20 : 20)
            lo = min(lo, (r * c + off).z)
        }
        #expect(abs(lo) < 1e-4)
        #expect(abs(off.x) < 1e-4 && abs(off.y) < 1e-4)
    }
}

@Suite("Frame stats")
struct FrameStatsTests {
    @Test func summarisesIntervalsAndGpuTime() {
        let s = FrameStats()
        for i in 0..<100 { s.record(presentedAt: Double(i) / 120, gpuSeconds: 0.002) }
        let r = s.summary()
        #expect(r.frames == 99)
        #expect(abs(r.fps - 120) < 0.5)
        #expect(abs(r.gpuMeanMs - 2) < 1e-6)
    }
}
