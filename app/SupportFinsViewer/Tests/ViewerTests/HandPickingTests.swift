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
        var clicks = 0, removeClicks = 0
        c.onHandClick = { _ in clicks += 1 }
        c.onSupportClick = { _ in removeClicks += 1 }
        c.click(x: 128, y: 128, width: 256, height: 256)
        try await eventually("a click") { clicks == 1 }
        #expect(removeClicks == 0)
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
}
