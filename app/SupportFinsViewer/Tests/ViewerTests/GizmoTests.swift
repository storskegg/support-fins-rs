import Metal
import Testing
import simd
@testable import Viewer

@Suite("Rotate gizmo (TransformControls)")
struct GizmoTests {
    let cam = OrbitCamera(target: [0, 0, 20], distance: 300, azimuth: -.pi / 3, elevation: 0.6, aspect: 1.5)
    let w: Float = 900
    let h: Float = 600
    let g = RotateGizmo(center: [0, 0, 20])

    func px(_ p: SIMD3<Float>) -> SIMD2<Float> { g.screen(p, camera: cam, width: w, height: h) }
    func ray(_ p: SIMD2<Float>) -> (origin: SIMD3<Float>, direction: SIMD3<Float>) {
        cam.ray(ndcX: 2 * p.x / w - 1, ndcY: 1 - 2 * p.y / h)
    }
    func drag(_ handle: GizmoHandle, from a: SIMD2<Float>) -> RotateGizmo.Drag? { g.beginDrag(handle, camera: cam, ray: ray(a)) }

    @Test func sizesAreThreeJsUnits() {
        // s = distance * 1.9 tan(fov / 2) * 0.85 / 4; the axis rings are 0.5 s, the outer ring 0.75 s
        let s = 300 * 1.9 * tan(cam.fovY / 2) * 0.85 / 4
        #expect(abs(g.unit(camera: cam) - s) < 1e-3)
        #expect(abs(g.radius(camera: cam) - 0.5 * s) < 1e-3)
        // constant on screen: 0.2019 of the view height per unit, whatever the distance
        var far = cam
        far.distance = 900
        #expect(abs(g.pixelsPerUnit(camera: cam, height: h) / h - 0.2019) < 0.001)
        #expect(abs(g.pixelsPerUnit(camera: far, height: h) - g.pixelsPerUnit(camera: cam, height: h)) < 0.05)
    }

    @Test func theRingsAreCirclesOfTheRightSizeAndPlane() {
        let s = g.unit(camera: cam)
        let toEye = simd_normalize(cam.eye - g.center)
        for axis in GizmoAxis.allCases {
            let pts = g.arc(axis, camera: cam)
            for p in pts {
                let v = p - g.center
                #expect(abs(simd_length(v) - 0.5 * s) < 1e-3 && abs(simd_dot(v, axis.vector)) < 1e-3)
                #expect(simd_dot(v, toEye) > -1e-3 * s)                       // the near half
            }
        }
        for (r, name) in [(RotateGizmo.screenRadius, "E"), (RotateGizmo.freeRadius, "XYZE")] {
            let pts = g.circle(radius: r, camera: cam)
            #expect(pts.first == pts.last || simd_length(pts.first! - pts.last!) < 1e-3, Comment(rawValue: name))
            for p in pts {
                let v = p - g.center
                #expect(abs(simd_length(v) - r * s) < 1e-3 && abs(simd_dot(v, toEye)) < 1e-3, Comment(rawValue: name))   // in the view plane
            }
        }
    }

    @Test func pickingFindsTheHandleUnderThePointer() {
        let axisMid = g.arc(.z, camera: cam)[RotateGizmo.segments / 2]
        #expect(g.hit(atPixel: px(axisMid), camera: cam, width: w, height: h) == .axis(.z))
        let e = g.circle(radius: RotateGizmo.screenRadius, camera: cam)[RotateGizmo.segments / 8]
        #expect(g.hit(atPixel: px(e), camera: cam, width: w, height: h) == .screen)
        // the middle: within 0.25 of the centre, even off the rings
        let c = px(g.center)
        #expect(g.hit(atPixel: c + [3, 2], camera: cam, width: w, height: h) == .free)
        // the pick tube is 0.1 either side of a ring, and no more
        let ppu = g.pixelsPerUnit(camera: cam, height: h)
        let out = simd_normalize(px(e) - c)
        #expect(g.hit(atPixel: px(e) + out * (0.08 * ppu), camera: cam, width: w, height: h) == .screen)
        #expect(g.hit(atPixel: px(e) + out * (0.2 * ppu), camera: cam, width: w, height: h) == nil)
        #expect(g.hit(atPixel: [5, 5], camera: cam, width: w, height: h) == nil)
    }

    @Test func theMiddleLightsEveryHandleAndAnAxisOnlyItself() {
        for h in [GizmoHandle.axis(.x), .axis(.y), .axis(.z), .screen, .free] {
            #expect(GizmoHandle.free.lights(h))
        }
        #expect(GizmoHandle.axis(.x).lights(.axis(.x)) && !GizmoHandle.axis(.x).lights(.axis(.y)) && !GizmoHandle.axis(.x).lights(.screen))
        #expect(GizmoHandle.screen.lights(.screen) && !GizmoHandle.screen.lights(.free))
        #expect(GizmoHandle.screen.deltaLabel == "free" && GizmoHandle.free.deltaLabel == "free" && GizmoHandle.axis(.z).deltaLabel == "Z")
    }

    /// Unit vector on screen along the tangent of an axis ring: the direction of `axis x eye` projected.
    func tangentPixels(_ axis: GizmoAxis) -> SIMD2<Float> {
        let e = simd_normalize(cam.eye - g.center)
        let t = simd_normalize(simd_cross(axis.vector, e))
        let a = px(g.center), b = px(g.center + t * 10)
        return simd_normalize(b - a)
    }

    @Test func anAxisRingTurnsByTheTangentialDisplacementAtTwentyOverDistance() throws {
        // ROTATION_SPEED = 20 / d, offset in world units on the plane facing the camera: radians per pixel is
        // 20 * (2 tan(fov / 2) / height), whatever the distance
        let perPixel = 20 * 2 * tan(cam.fovY / 2) / h
        for axis in GizmoAxis.allCases {
            let start = px(g.arc(axis, camera: cam)[RotateGizmo.segments / 2])
            let d = try #require(drag(.axis(axis), from: start))
            let dir = tangentPixels(axis)
            for dist: Float in [10, 40, -25] {
                let t = try #require(g.turn(d, ray: ray(start + dir * dist), snap: false))
                #expect(abs(t.axis - axis.vector).max() < 1e-5)
                #expect(abs(Float(t.radians) - dist * perPixel) < 0.01 * abs(dist) * perPixel + 1e-4, "\(axis) \(dist): \(t.radians)")
            }
            // across the tangent (along the ring's radius on screen) turns nothing
            let across = SIMD2<Float>(-dir.y, dir.x)
            let none = try #require(g.turn(d, ray: ray(start + across * 30), snap: false))
            #expect(abs(none.radians) < 0.02)
        }
    }

    @Test func theSnapRoundsTheFinalAngleToFiveDegrees() throws {
        let start = px(g.arc(.y, camera: cam)[RotateGizmo.segments / 2])
        let d = try #require(drag(.axis(.y), from: start))
        let to = start + tangentPixels(.y) * 33
        let free = try #require(g.turn(d, ray: ray(to), snap: false)).radians * 180 / .pi
        let snapped = try #require(g.turn(d, ray: ray(to), snap: true)).radians * 180 / .pi
        #expect(abs(snapped - (free / 5).rounded() * 5) < 1e-9 && snapped.truncatingRemainder(dividingBy: 5) == 0)
        #expect(free != snapped)
    }

    @Test func theMiddleIsATrackballThePointerDragsTheFrontSurfaceAlong() throws {
        let c = px(g.center)
        let d = try #require(drag(.free, from: c + [2, 0]))
        let to = c + [2, 0] + [8, -4]                                      // right and up on the screen
        let t = try #require(g.turn(d, ray: ray(to), snap: false))
        let e = simd_normalize(cam.eye - g.center)
        // the point of the ball facing the camera moves the way the pointer moved
        let front = e
        let moved = RotateGizmo.rotation(axis: t.axis, radians: t.radians) * SIMD3<Double>(SIMD3<Double>(front)) - SIMD3<Double>(front)
        let screenDir = SIMD3<Double>(SIMD3<Double>(cam.basis.right) * 8 + SIMD3<Double>(cam.basis.up) * 4)
        #expect(simd_dot(simd_normalize(moved), simd_normalize(screenDir)) > 0.97)
        // angle = |offset| * 20 / d, in radians (unsnapped)
        let perPixel = 20 * 2 * tan(cam.fovY / 2) / h
        let len = simd_length(SIMD2<Float>(8, 4))
        #expect(abs(Float(abs(t.radians)) - len * perPixel) < 0.02 * len * perPixel)
        // a zero move turns nothing
        let still = try #require(g.turn(d, ray: ray(c + [2, 0]), snap: true))
        #expect(still.radians == 0)
    }

    @Test func theOuterRingTurnsAboutTheViewByTheAngleSweptRoundTheCentre() throws {
        let c = px(g.center)
        let ppu = g.pixelsPerUnit(camera: cam, height: h)
        let r = RotateGizmo.screenRadius * ppu
        func at(_ degrees: Float) -> SIMD2<Float> {
            let t = degrees * .pi / 180
            return c + [r * cos(t), -r * sin(t)]                             // counter-clockwise on the screen
        }
        let d = try #require(drag(.screen, from: at(10)))
        for sweep: Float in [30, 95, -60] {
            let t = try #require(g.turn(d, ray: ray(at(10 + sweep)), snap: false))
            let e = simd_normalize(cam.eye - g.center)
            #expect(abs(t.axis - e).max() < 1e-4)
            #expect(abs(Float(t.radians) * 180 / .pi - sweep) < 0.6, "swept \(sweep): \(t.radians * 180 / .pi)")
        }
        let s = try #require(g.turn(d, ray: ray(at(10 + 33)), snap: true))
        #expect((s.radians * 180 / .pi).truncatingRemainder(dividingBy: 5) == 0)
    }

    @Test func lookingDownAnAxisTheRingTurnsInThePlaneOfTheView() throws {
        var top = cam
        top.azimuth = 0
        top.elevation = OrbitCamera.maxElevation
        let g = RotateGizmo(center: [0, 0, 20])
        func ray2(_ p: SIMD2<Float>) -> (origin: SIMD3<Float>, direction: SIMD3<Float>) {
            top.ray(ndcX: 2 * p.x / 900 - 1, ndcY: 1 - 2 * p.y / 600)
        }
        let c = g.screen(g.center, camera: top, width: 900, height: 600)
        let a = c + [80, 0], b = c + [0, -80]                                 // a quarter turn counter-clockwise
        let d = try #require(g.beginDrag(.axis(.z), camera: top, ray: ray2(a)))
        let t = try #require(g.turn(d, ray: ray2(b), snap: false))
        #expect(abs(t.radians * 180 / .pi - 90) < 0.5)
        #expect(simd_dot(t.axis, [0, 0, 1]) > 0.999)
    }

    @Test func axisLineHidesWhenTheAxisPointsDownTheView() {
        var top = cam
        top.elevation = OrbitCamera.maxElevation
        #expect(!g.showsAxisLine(.z, camera: top))
        #expect(g.showsAxisLine(.x, camera: top) && g.showsAxisLine(.y, camera: top))
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

    func ringPoint(_ c: ViewerController, _ axis: GizmoAxis, _ i: Int = 0) -> (Int, Int) {
        // the point of the arc farthest from the middle on screen, clear of the trackball's pick sphere
        let mid = pixelOf(c, c.gizmoCenter)
        let pts = RotateGizmo(center: c.gizmoCenter).arc(axis, camera: c.renderer.camera).map { pixelOf(c, $0) }
        return pts.max { hypot(Double($0.0 - mid.0), Double($0.1 - mid.1)) < hypot(Double($1.0 - mid.0), Double($1.1 - mid.1)) }!
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

    @Test func theRingsDrawInTheirAxisColoursAndTheOuterRingIsFaintYellow() throws {
        let (c, _) = try rig()
        let px = pixels(c)
        #expect(has(px, near: ringPoint(c, .x)) { r, g, b in r > 180 && g < 90 && b < 90 }, "no red ring")
        #expect(has(px, near: ringPoint(c, .y)) { r, g, b in g > 180 && r < 90 && b < 90 }, "no green ring")
        #expect(has(px, near: ringPoint(c, .z)) { r, g, b in b > 180 && r < 90 && g < 90 }, "no blue ring")
        let g = RotateGizmo(center: c.gizmoCenter)
        let e = pixelOf(c, g.circle(radius: RotateGizmo.screenRadius, camera: c.renderer.camera)[RotateGizmo.segments / 3])
        // 25% yellow over the dark background: a dim olive, clearly lighter than the background
        #expect(has(px, near: e) { r, g, b in r > 55 && g > 55 && b < 50 }, "no outer ring")
        let free = pixelOf(c, g.circle(radius: RotateGizmo.freeRadius, camera: c.renderer.camera)[RotateGizmo.segments / 3])
        #expect(has(px, near: free) { r, g, b in abs(r - 120) < 25 && abs(g - 120) < 25 && abs(b - 120) < 25 }, "no gray trackball outline")
    }

    @Test func hidingTheRingsRemovesThemAndTheirGrab() throws {
        let (c, _) = try rig()
        let spot = ringPoint(c, .x)
        #expect(c.ringAt(x: spot.0, y: spot.1, width: size, height: size) == .axis(.x))
        c.showRings = false
        #expect(c.ringAt(x: spot.0, y: spot.1, width: size, height: size) == nil)
        #expect(c.renderer.gizmo == nil)
    }

    @Test func aHoveredRingTurnsYellowAndTheMiddleLightsThemAll() throws {
        let (c, _) = try rig()
        let spot = ringPoint(c, .y)
        c.pointerMoved(x: spot.0, y: spot.1, width: size, height: size)
        #expect(c.ringHover == .axis(.y))
        var px = pixels(c)
        #expect(has(px, near: spot) { r, g, b in r > 200 && g > 200 && b < 90 }, "hovered ring is not yellow")
        #expect(has(px, near: ringPoint(c, .z)) { r, g, b in b > 180 && r < 90 && g < 90 }, "another ring changed colour")
        // the middle: every ring lit
        let mid = pixelOf(c, c.gizmoCenter)
        c.pointerMoved(x: mid.0 + 2, y: mid.1 + 2, width: size, height: size)
        #expect(c.ringHover == .free)
        px = pixels(c)
        for axis in GizmoAxis.allCases {
            #expect(has(px, near: ringPoint(c, axis)) { r, g, b in r > 200 && g > 200 && b < 90 }, "\(axis) not lit by XYZE")
        }
        c.clearRingHover()
        c.ringFade = 0.15
        px = pixels(c)
        #expect(!has(px, near: ringPoint(c, .z)) { r, g, b in b > 150 && r < 70 && g < 70 })
    }

    @Test func aDragThroughTheControllerTurnsThePartAboutTheGrabbedCentreInFiveDegreeSteps() async throws {
        let (c, _) = try rig()
        var events: [String] = []
        var last: (Double, GizmoHandle)?
        c.onRing = { e in
            switch e {
            case .began(let h): events.append("began \(h.name)")
            case .turned(_, let deg, let h): events.append("turned"); last = (deg, h)
            case .ended(let t): events.append("ended \(t)")
            }
        }
        let center = c.gizmoCenter
        let start = ringPoint(c, .z)
        #expect(c.beginRingDrag(x: start.0, y: start.1, width: size, height: size))
        #expect(c.ringDragging == .axis(.z))
        // drag along the ring's tangent on screen
        let cam = c.renderer.camera
        let e = simd_normalize(cam.eye - center)
        let t = simd_normalize(simd_cross(GizmoAxis.z.vector, e))
        let a = c.renderer.camera.project(center), b = c.renderer.camera.project(center + t * 10)
        let dir = simd_normalize(SIMD2<Float>(b.x - a.x, -(b.y - a.y)))
        let to = (start.0 + Int(dir.x * 60), start.1 + Int(dir.y * 60))
        c.dragRing(x: to.0, y: to.1, width: size, height: size, snap: true)
        let (deg, handle) = try #require(last)
        #expect(handle == .axis(.z) && deg != 0 && deg.truncatingRemainder(dividingBy: 5) == 0, "\(deg)")
        #expect(c.rotation != matrix_identity_float3x3)
        #expect(simd_length(c.gizmoCenter - center) < 0.05)                    // the centre holds still
        c.endRingDrag()
        #expect(events.first == "began Z" && events.last == "ended true" && c.ringDragging == nil)
        // re-seated on the plate on release
        var lo = Float.infinity
        for i in 0..<8 {
            let corner = SIMD3<Float>(i & 1 == 0 ? -20 : 20, i & 2 == 0 ? -20 : 20, i & 4 == 0 ? -20 : 20)
            lo = min(lo, (c.renderer.rotation * corner + c.renderer.offset).z)
        }
        #expect(abs(lo) < 1e-3)
    }

    @Test func theMiddleAndTheOuterRingStartDragsToo() throws {
        let (c, _) = try rig()
        let mid = pixelOf(c, c.gizmoCenter)
        #expect(c.beginRingDrag(x: mid.0 + 3, y: mid.1, width: size, height: size))
        #expect(c.ringDragging == .free)
        c.dragRing(x: mid.0 + 60, y: mid.1 - 20, width: size, height: size, snap: false)
        #expect(c.rotation != matrix_identity_float3x3)
        c.endRingDrag()
        let g = RotateGizmo(center: c.gizmoCenter)
        let e = pixelOf(c, g.circle(radius: RotateGizmo.screenRadius, camera: c.renderer.camera)[10])
        #expect(c.beginRingDrag(x: e.0, y: e.1, width: size, height: size))
        #expect(c.ringDragging == .screen)
        c.endRingDrag()
    }

    @Test func aPressOffTheRingsLeavesThePointerToTheCamera() throws {
        let (c, _) = try rig()
        #expect(!c.beginRingDrag(x: 2, y: 2, width: size, height: size))
        var ended: Bool?
        c.onRing = { if case .ended(let t) = $0 { ended = t } }
        let s = ringPoint(c, .x)
        #expect(c.beginRingDrag(x: s.0, y: s.1, width: size, height: size))
        c.endRingDrag()
        #expect(ended == false)
        #expect(c.rotation == matrix_identity_float3x3)
    }
}
