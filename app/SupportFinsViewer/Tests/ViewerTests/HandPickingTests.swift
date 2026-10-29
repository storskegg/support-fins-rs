import Metal
import Testing
import simd
@testable import Viewer

/// The viewer's hand-placement pointer (`web/ui/walls.js` `drawActive` / `braceActive`, `web/app.js` dispatch): the
/// face under the pointer and the ray through it go to the hooks; Draw hides the rotate rings, the sway-brace mode keeps
/// them; the keys and Esc are the mode's.
@MainActor
@Suite("Hand picking")
struct HandPickingTests {
    func rig() throws -> ViewerController {
        let c = try #require(ViewerController(), "no Metal device")
        c.show(GeneratedMesh.cube(edge: 40, device: c.renderer.device), name: "cube")
        c.renderer.camera = OrbitCamera(target: [0, 0, 20], distance: 200, azimuth: 0, elevation: OrbitCamera.maxElevation, aspect: 1)
        return c
    }

    func eventually(_ what: String, _ ok: () -> Bool) async throws {
        for _ in 0..<400 where !ok() { try await Task.sleep(for: .milliseconds(5)) }
        #expect(ok(), "\(what)")
    }

    @Test func hoverReportsTheFaceAndTheRay() async throws {
        let c = try rig()
        c.handPicking = .walls
        var got: ViewerController.HandPointer?
        c.onHandMove = { got = $0 }
        c.hover(x: 128, y: 128, width: 256, height: 256)
        try await eventually("a pointer") { got != nil }
        let p = try #require(got)
        #expect(p.face?.part == 0 && [2, 3].contains(p.face?.face ?? -1), "the cube's top, seen from above")
        // looking straight down at the middle: the ray runs down the z axis
        #expect(abs(p.direction.z + 1) < 1e-3 && abs(p.origin.x) < 1 && abs(p.origin.y) < 1 && p.origin.z > 100)
        #expect(abs(p.direction.x) < 1e-3 && abs(simd_length(p.direction) - 1) < 1e-6)
        #expect(p.size == CGSize(width: 256 / CGFloat(c.renderer.pixelScale), height: 256 / CGFloat(c.renderer.pixelScale)))
        #expect(abs(p.location.x - 128 / CGFloat(c.renderer.pixelScale)) < 1e-6)
        // the part's face is not lit by a hand mode (only Lay a face flat lights it)
        #expect(c.renderer.hoverFace == nil)
        // off the part: no face, a ray all the same
        got = nil
        c.hover(x: 4, y: 4, width: 256, height: 256)
        try await eventually("a pointer off the part") { got != nil }
        #expect(got?.face == nil)
    }

    @Test func aClickGoesToTheHookAndRemoveFinsKeepsItsPriority() async throws {
        let c = try rig()
        c.handPicking = .walls
        var clicks = 0, removeClicks = 0, taps = 0
        c.onHandClick = { _ in clicks += 1 }
        c.onSupportClick = { _ in removeClicks += 1 }
        c.onTap = { _, _, _, _ in taps += 1 }
        c.click(x: 128, y: 128, width: 256, height: 256)
        try await eventually("a click") { clicks == 1 }
        #expect(removeClicks == 0)
        #expect(taps == 1, "the plates read the click too (a parked model, another plate)")
        // Move owns the pointer: no wall point while it is on, the plates still read the click
        c.move.setOn(true)
        c.click(x: 128, y: 128, width: 256, height: 256)
        try await Task.sleep(for: .milliseconds(100))
        #expect(clicks == 1 && taps == 2, "Move on: no hand click")
        c.move.setOn(false)
        c.handPicking = .off
        c.click(x: 128, y: 128, width: 256, height: 256)
        try await Task.sleep(for: .milliseconds(100))
        #expect(clicks == 1, "no hand mode: no click")
    }

    @Test func theRingsStepAsideForDrawAndStayForBraces() throws {
        let c = try rig()
        c.showRings = true
        #expect(c.renderer.gizmo != nil)
        c.handPicking = .walls
        #expect(c.renderer.gizmo == nil && c.ringAt(x: 0, y: 0, width: 256, height: 256) == nil)
        c.handPicking = .braces
        #expect(c.renderer.gizmo != nil, "the rings stay in the sway-brace mode")
        c.ringFade = 0.15
        #expect(c.renderer.gizmo?.fade == 0.15)
        c.handPicking = .off
        #expect(c.renderer.gizmo != nil)
    }

    @Test func escAndRightClickAreTheModesAndTheKeyIsTold() throws {
        let c = try rig()
        var calls: [Bool] = []
        c.onHandCancel = { escape in calls.append(escape); return escape }
        #expect(!c.cancelArmed(escape: true), "no hand mode: not ours")
        c.handPicking = .walls
        #expect(c.cancelArmed(escape: true) && !c.cancelArmed())
        #expect(calls == [true, false])
        var left = 0
        c.onHandMove = { if $0 == nil { left += 1 } }
        c.handPointerLeft()
        #expect(left == 1)
        c.handPicking = .off
        c.handPointerLeft()
        #expect(left == 1)
    }

    @Test func theCrosshairFollowsTheModeAndGoesWithIt() throws {
        let c = try rig()
        c.handPicking = .walls
        c.handCursor = true
        #expect(c.handCursor)
        c.handPicking = .off
        #expect(!c.handCursor)
    }

    /// DrawUITests clicks the `viewport` element at (0.38, 0.32) and (0.62, 0.32) of its frame after View > Bottom, on
    /// the UI tests' fixture (`PartWindowUITestCase.writeSTL`: a 10 mm stem, z 0-20, under a 60 mm slab, z 20-24). Those
    /// points must land on the slab's underside, clear of the stem, for any window shape the test can get: the default
    /// 1100 x 760 window less an inspector of 240-360 pt, and wider or narrower ones besides.
    @Test func theDrawUITestsClicksLandOnTheSlabsUnderside() async throws {
        let c = try #require(ViewerController(), "no Metal device")
        func box(_ x0: Float, _ x1: Float, _ y0: Float, _ y1: Float, _ z0: Float, _ z1: Float) -> [SIMD3<Float>] {
            let v: [SIMD3<Float>] = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]]
            let faces = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]]
            return faces.flatMap { f in f.map { v[$0] } }
        }
        let tris = box(-5, 5, -5, 5, 0, 20) + box(-30, 30, -30, 30, 20, 24)
        let normals = stride(from: 0, to: tris.count, by: 3).map { simd_normalize(simd_cross(tris[$0 + 1] - tris[$0], tris[$0 + 2] - tris[$0])) }
        let slabUnderside = [12, 13]
        #expect(normals[12].z < -0.99 && normals[13].z < -0.99, "faces 12 and 13 are the slab's underside")
        let height = 760
        for width in [500, 740, 820, 860, 1100, 1400] {
            c.renderer.camera.aspect = Float(width) / Float(height)
            c.show(GeneratedMesh(triangles: tris, faceNormals: normals, device: c.renderer.device), name: "bracket")
            c.apply(.bottom)
            try await Task.sleep(for: .milliseconds(Int(CameraGlide.seconds * 1000) + 150))
            for nx in [0.38, 0.62] {
                let x = Int(nx * Double(width)), y = Int(0.32 * Double(height))
                let face = await c.renderer.pickFace(x: x, y: y, width: width, height: height)
                #expect(face.map { slabUnderside.contains($0) } == true, "\(width) x \(height) at (\(nx), 0.32): face \(String(describing: face))")
                // and well inside it: at least 3 mm from the slab's edge and from the stem's shadow
                let r = c.ndcRay(x: x, y: y, width: width, height: height)
                let t = (20 - r.origin.z) / r.direction.z
                let p = r.origin + r.direction * t
                #expect(abs(p.x) < 27 && abs(p.y) < 27, "\(width) x \(height): \(p) on the underside")
                let s = (0 - r.origin.z) / r.direction.z
                let q = r.origin + r.direction * s
                #expect(max(abs(q.x), abs(q.y)) > 8, "\(width) x \(height): the ray passes the stem at \(q)")
            }
        }
    }
}
