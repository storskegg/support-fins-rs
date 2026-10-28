import Metal
import Testing
import simd
@testable import Viewer

@Suite("Rotate gizmo")
struct GizmoTests {
    let cam = OrbitCamera(target: [0, 0, 20], distance: 300, azimuth: -.pi / 3, elevation: 0.6, aspect: 1.5)
    let w: Float = 900
    let h: Float = 600

    func px(_ p: SIMD3<Float>, _ g: RotateGizmo) -> SIMD2<Float> { g.screen(p, camera: cam, width: w, height: h) }
    func ray(at p: SIMD2<Float>) -> (origin: SIMD3<Float>, direction: SIMD3<Float>) {
        cam.ray(ndcX: 2 * p.x / w - 1, ndcY: 1 - 2 * p.y / h)
    }

    @Test func radiusIsTheThreeJsFormulaAndConstantOnScreen() {
        let g = RotateGizmo(center: [0, 0, 20])
        // factor = distance * 1.9 tan(fov / 2); radius = factor * 0.85 / 4
        let want = 300 * 1.9 * tan(cam.fovY / 2) * 0.85 / 4
        #expect(abs(g.radius(camera: cam) - want) < 1e-3)
        // twice as far: twice the world radius, the same size on screen
        var far = cam
        far.distance = 600
        let r1 = g.radius(camera: cam), r2 = g.radius(camera: far)
        #expect(abs(r2 / r1 - 2) < 1e-3)
        // ... and about 17% of the distance, i.e. a ring of ~40% of half the view height
        #expect(abs(r1 / 300 - 0.167) < 0.002)
    }

    @Test func eachRingIsTheNearHalfOfACircleInItsPlane() {
        let g = RotateGizmo(center: [5, -3, 20])
        let r = g.radius(camera: cam)
        let toEye = simd_normalize(cam.eye - g.center)
        for axis in GizmoAxis.allCases {
            let pts = g.arc(axis, camera: cam)
            #expect(pts.count == RotateGizmo.segments + 1)
            for p in pts {
                let v = p - g.center
                #expect(abs(simd_length(v) - r) < 1e-3)
                #expect(abs(simd_dot(v, axis.vector)) < 1e-3)               // in the plane
                #expect(simd_dot(v, toEye) > -1e-3 * r)                      // the half facing the camera
            }
            // a half turn from end to end
            let a = pts.first! - g.center, b = pts.last! - g.center
            #expect(abs(simd_dot(a, b) / (r * r) + 1) < 1e-3)
        }
    }

    @Test func aPixelOnARingGrabsItAndAFarOneDoesNot() {
        let g = RotateGizmo(center: [0, 0, 20])
        for axis in GizmoAxis.allCases {
            let mid = g.arc(axis, camera: cam)[RotateGizmo.segments / 2]
            #expect(g.hit(atPixel: px(mid, g), camera: cam, width: w, height: h) == axis)
            // a few pixels off still grabs; many do not
            #expect(g.hit(atPixel: px(mid, g) + [4, 3], camera: cam, width: w, height: h) != nil)
        }
        #expect(g.hit(atPixel: [5, 5], camera: cam, width: w, height: h) == nil)
        // the centre of the view is inside all three rings, on none
        #expect(g.hit(atPixel: px(g.center, g), camera: cam, width: w, height: h) == nil)
    }

    @Test func theRingNearestThePointerWins() {
        let g = RotateGizmo(center: [0, 0, 20])
        let z = g.arc(.z, camera: cam)[RotateGizmo.segments / 4]
        #expect(g.hit(atPixel: px(z, g), camera: cam, width: w, height: h) == .z)
        // restricting the candidates leaves the others out
        #expect(g.hit(axes: [.x], atPixel: px(z, g), camera: cam, width: w, height: h) != .z)
    }

    @Test func draggingAroundTheRingGivesTheAngleSnappedTo5Degrees() throws {
        let g = RotateGizmo(center: [0, 0, 20])
        let r = g.radius(camera: cam)
        for axis in GizmoAxis.allCases {
            let (u, wv) = axis.basis
            func point(_ degrees: Float) -> SIMD3<Float> {
                let t = degrees * .pi / 180
                return g.center + r * (cos(t) * u + sin(t) * wv)
            }
            let drag = try #require(g.beginDrag(axis, ray: ray(at: px(point(10), g))))
            for (turn, snapped) in [(30.0, 30.0), (32.0, 30.0), (33.0, 35.0), (-47.0, -45.0), (120.0, 120.0)] {
                let free = try #require(g.angle(drag, ray: ray(at: px(point(10 + Float(turn)), g)), snap: false))
                #expect(abs(free - turn) < 0.05, "\(axis) free \(free) vs \(turn)")
                let s = try #require(g.angle(drag, ray: ray(at: px(point(10 + Float(turn)), g)), snap: true))
                #expect(abs(s - snapped) < 1e-9, "\(axis) snapped \(s) vs \(snapped)")
            }
        }
    }

    @Test func theDragAngleFollowsTheRightHandRule() throws {
        let g = RotateGizmo(center: [0, 0, 0])
        var top = OrbitCamera(target: .zero, distance: 300, azimuth: 0, elevation: OrbitCamera.maxElevation, aspect: 1)
        top.aspect = 1
        // looking straight down at a Z ring: moving the pointer counter-clockwise on screen is a positive turn
        let r = g.radius(camera: top)
        func ndc(_ p: SIMD3<Float>) -> (origin: SIMD3<Float>, direction: SIMD3<Float>) {
            let n = top.project(p)
            return top.ray(ndcX: n.x, ndcY: n.y)
        }
        let d = try #require(g.beginDrag(.z, ray: ndc([r, 0, 0])))
        let a = try #require(g.angle(d, ray: ndc([0, r, 0]), snap: false))
        #expect(abs(a - 90) < 0.1)
        let rot = RotateGizmo.rotation(.z, degrees: a)
        #expect(simd_length(rot * SIMD3<Double>(1, 0, 0) - SIMD3(0, 1, 0)) < 1e-6)
    }

    @Test func anEdgeOnPlaneGivesNoAngle() {
        let g = RotateGizmo(center: [0, 0, 0])
        // a ray parallel to the Z ring's plane never meets it
        let flat = (origin: SIMD3<Float>(-100, 0, 5), direction: SIMD3<Float>(1, 0, 0))
        #expect(g.beginDrag(.z, ray: flat) == nil)
    }
}

@MainActor
@Suite("Rotate gizmo in the viewer")
struct GizmoViewerTests {
    let size = 400

    func rig() throws -> (ViewerController, GeneratedMesh) {
        let c = try #require(ViewerController(), "no Metal device")
        let m = GeneratedMesh.cube(edge: 40, device: c.renderer.device)
        c.show(m, name: "cube")
        c.renderer.camera = OrbitCamera(target: [0, 0, 20], distance: 220, azimuth: -.pi / 3, elevation: 0.5, aspect: 1)
        return (c, m)
    }

    func pixelOf(_ c: ViewerController, _ p: SIMD3<Float>) -> (Int, Int) {
        let n = c.renderer.camera.project(p)
        return (Int((n.x * 0.5 + 0.5) * Float(size)), Int((0.5 - n.y * 0.5) * Float(size)))
    }

    func ringPoint(_ c: ViewerController, _ axis: GizmoAxis, _ i: Int = RotateGizmo.segments / 2) -> (Int, Int) {
        pixelOf(c, RotateGizmo(center: c.gizmoCenter).arc(axis, camera: c.renderer.camera)[i])
    }

    func pixels(_ c: ViewerController) -> [UInt8] {
        ViewerRenderer.pixels(of: c.renderer.renderOffscreen(width: size, height: size))
    }

    func has(_ px: [UInt8], near p: (Int, Int), _ test: (Int, Int, Int) -> Bool) -> Bool {
        for dy in -3...3 { for dx in -3...3 {
            let x = p.0 + dx, y = p.1 + dy
            guard x >= 0, y >= 0, x < size, y < size else { continue }
            let o = (y * size + x) * 4
            if test(Int(px[o + 2]), Int(px[o + 1]), Int(px[o])) { return true }
        } }
        return false
    }

    @Test func theRingsDrawInTheirAxisColours() throws {
        let (c, _) = try rig()
        let px = pixels(c)
        #expect(has(px, near: ringPoint(c, .x)) { r, g, b in r > 180 && g < 90 && b < 90 }, "no red ring")
        #expect(has(px, near: ringPoint(c, .y)) { r, g, b in g > 180 && r < 90 && b < 90 }, "no green ring")
        #expect(has(px, near: ringPoint(c, .z)) { r, g, b in b > 180 && r < 90 && g < 90 }, "no blue ring")
    }

    @Test func hidingTheRingsRemovesThemAndTheirGrab() throws {
        let (c, _) = try rig()
        let spot = ringPoint(c, .x)
        #expect(c.ringAt(x: spot.0, y: spot.1, width: size, height: size) == .x)
        c.showRings = false
        let px = pixels(c)
        #expect(!has(px, near: spot) { r, g, b in r > 180 && g < 90 && b < 90 })
        #expect(c.ringAt(x: spot.0, y: spot.1, width: size, height: size) == nil)
        #expect(c.renderer.gizmo == nil)
    }

    @Test func aHoveredRingTurnsYellowAndTheFadeDimsThem() throws {
        let (c, _) = try rig()
        let spot = ringPoint(c, .y)
        c.pointerMoved(x: spot.0, y: spot.1, width: size, height: size)
        #expect(c.ringHover == .y)
        var px = pixels(c)
        #expect(has(px, near: spot) { r, g, b in r > 200 && g > 200 && b < 90 }, "hovered ring is not yellow")
        c.clearRingHover()
        c.ringFade = 0.15
        px = pixels(c)
        #expect(!has(px, near: ringPoint(c, .z)) { r, g, b in b > 150 && r < 70 && g < 70 }, "faded ring is still bright")
        #expect(has(px, near: ringPoint(c, .z)) { r, g, b in b > 40 && b > r + 15 }, "the faded ring vanished")
    }

    @Test func dragAlongOneRingTurnsThePartAboutItsCentreInFiveDegreeSteps() async throws {
        let (c, _) = try rig()
        var events: [String] = []
        var deltas: [simd_double3x3] = []
        c.onRing = { e in
            switch e {
            case .began(let a): events.append("began \(a.label)")
            case .turned(let d, let deg, _): events.append("turned \(Int(deg))"); deltas.append(d)
            case .ended(let t): events.append("ended \(t)")
            }
        }
        let center = c.gizmoCenter
        let start = ringPoint(c, .z, RotateGizmo.segments / 2)
        #expect(c.beginRingDrag(x: start.0, y: start.1, width: size, height: size))
        #expect(c.ringDragging == .z)
        // drag a quarter of the way round the ring's centre on screen: some positive multiple of 5
        let g = RotateGizmo(center: center)
        let r = g.radius(camera: c.renderer.camera)
        let (u, w) = GizmoAxis.z.basis
        let a0 = atan2(simd_dot(g.arc(.z, camera: c.renderer.camera)[RotateGizmo.segments / 2] - center, w),
                       simd_dot(g.arc(.z, camera: c.renderer.camera)[RotateGizmo.segments / 2] - center, u))
        let to = pixelOf(c, center + r * (cos(a0 + 0.5236) * u + sin(a0 + 0.5236) * w))   // +30 degrees
        c.dragRing(x: to.0, y: to.1, width: size, height: size, snap: true)
        #expect(abs(c.ringDegrees - 30) <= 5 && c.ringDegrees.truncatingRemainder(dividingBy: 5) == 0, "\(c.ringDegrees)")
        // the part has turned, about the centre it was grabbed at (the pivot holds still while dragging)
        #expect(c.rotation != matrix_identity_float3x3)
        #expect(simd_length(c.gizmoCenter - center) < 0.05)
        c.endRingDrag()
        #expect(c.ringDragging == nil && events.first == "began Z" && events.last == "ended true")
        #expect(deltas.count == 1)
        // released: the part is seated on the plate again
        var lo = Float.infinity
        let r3 = c.renderer.rotation, off = c.renderer.offset
        for i in 0..<8 {
            let corner = SIMD3<Float>(i & 1 == 0 ? -20 : 20, i & 2 == 0 ? -20 : 20, i & 4 == 0 ? -20 : 20)
            lo = min(lo, (r3 * corner + off).z)
        }
        #expect(abs(lo) < 1e-3)
    }

    @Test func aPressOffTheRingsLeavesThePointerToTheCamera() throws {
        let (c, _) = try rig()
        #expect(!c.beginRingDrag(x: 2, y: 2, width: size, height: size))
        #expect(c.ringDragging == nil)
        // a press on a ring that never moves is a drag that turned nothing
        var ended: Bool?
        c.onRing = { if case .ended(let t) = $0 { ended = t } }
        let s = ringPoint(c, .x)
        #expect(c.beginRingDrag(x: s.0, y: s.1, width: size, height: size))
        c.endRingDrag()
        #expect(ended == false)
        #expect(c.rotation == matrix_identity_float3x3)
    }
}
