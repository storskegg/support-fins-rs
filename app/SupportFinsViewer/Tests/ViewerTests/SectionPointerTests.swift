import Metal
import Testing
import simd
@testable import Viewer

/// The section plane's handle under the pointer (`web/ui/section.js` grips and sheet): the arrow pushes, the rings
/// tilt in 5 degree steps (Shift frees them), the sheet pushes where it is clear of the part, Shift + scroll pushes.
@MainActor
@Suite("Section handle")
struct SectionPointerTests {
    let w = 512, h = 512

    func rig(azimuth: Float = -.pi / 2) throws -> (ViewerRenderer, SectionTool) {
        let r = try #require(ViewerRenderer(), "no Metal device")
        let cube = GeneratedMesh.cube(edge: 40, device: r.device)
        r.setMesh(cube)
        r.setPose(rotation: matrix_identity_float3x3, offset: seatingOffset(bounds: cube.bounds, rotation: matrix_identity_float3x3))
        r.camera = OrbitCamera(target: [0, 0, 20], distance: 100, azimuth: 0, elevation: 0, aspect: 1)
        r.pixelScale = 1
        r.section.setOn(true)                                   // from +X: the plane's normal is -X
        r.camera = OrbitCamera(target: [0, 0, 20], distance: 100, azimuth: azimuth, elevation: 0.3, aspect: 1)
        return (r, r.section)
    }

    func pixel(_ r: ViewerRenderer, _ p: SIMD3<Float>) -> (x: Int, y: Int) {
        let g = SectionGizmo(plane: r.section.plane, camera: r.camera)
        let s = g.screen(p, width: Float(w), height: Float(h))
        return (Int(s.x.rounded()), Int(s.y.rounded()))
    }

    @Test func theArrowIsGrabbedOnItsShaftAndPushesAlongTheNormal() throws {
        let (r, tool) = try rig()
        let g = try #require(tool.gizmo)
        let n = tool.plane.normal, c = tool.plane.position
        let on = pixel(r, c + n * (0.35 * g.unit()))
        #expect(g.hit(atPixel: SIMD2(Float(on.x), Float(on.y)), width: Float(w), height: Float(h)) == .arrow)
        #expect(tool.begin(x: on.x, y: on.y, width: w, height: h))
        #expect(tool.busy && tool.dragging == .arrow)
        let to = pixel(r, c + n * (0.35 * g.unit() + 8))
        tool.dragged(x: to.x, y: to.y, width: w, height: h, snap: true)
        let moved = tool.plane.position - c
        #expect(abs(simd_dot(moved, n) - 8) < 0.5, "pushed \(simd_dot(moved, n)) mm")
        #expect(simd_length(moved - n * simd_dot(moved, n)) < 1e-3, "only along the normal")
        tool.ended()
        #expect(tool.dragging == nil)
    }

    @Test func aRingTiltsInFiveDegreeStepsAndShiftFreesIt() throws {
        for snap in [true, false] {
            let (r, tool) = try rig()
            let g = try #require(tool.gizmo)
            let arc = g.arc(.ringX)
            let start = pixel(r, arc[arc.count / 2])
            #expect(tool.begin(x: start.x, y: start.y, width: w, height: h))
            #expect(tool.dragging == .ringX || tool.dragging == .arrow || tool.dragging == .ringY || tool.dragging == .screen)
            let before = tool.plane.normal
            tool.dragged(x: start.x + 23, y: start.y - 17, width: w, height: h, snap: snap)
            let turned = acos(min(1, max(-1, simd_dot(before, tool.plane.normal)))) * 180 / .pi
            if tool.dragging == .ringX {
                if snap { #expect(abs(turned / 5 - (turned / 5).rounded()) < 0.02, "\(turned) degrees is not a multiple of 5") }
                else { #expect(abs(turned / 5 - (turned / 5).rounded()) > 0.001 || turned == 0) }
            }
            tool.ended()
        }
    }

    @Test func theSheetPushesWhereItIsClearOfThePartAndItsHandle() throws {
        let (r, tool) = try rig(azimuth: -.pi / 2 + 0.9)
        let c = tool.plane.position, n = tool.plane.normal
        // a point on the sheet 35 mm off the middle, outside the 40 mm cube
        let on = pixel(r, c + tool.plane.axisY * 35)
        tool.overPart = false
        tool.pointerMoved(x: on.x, y: on.y, width: w, height: h)
        #expect(tool.hot, "the sheet lights while a press would grab it")
        // over the part it is the part's
        tool.overPart = true
        #expect(!tool.hot)
        tool.overPart = false
        #expect(tool.begin(x: on.x, y: on.y, width: w, height: h))
        #expect(tool.sheetDragging && tool.busy)
        let p0 = c + tool.plane.axisY * 35
        let to = pixel(r, p0 + n * 10)
        tool.dragged(x: to.x, y: to.y, width: w, height: h, snap: true)
        #expect(abs(simd_dot(tool.plane.position - c, n) - 10) < 0.7, "pushed \(simd_dot(tool.plane.position - c, n)) mm")
        tool.ended()
        #expect(!tool.sheetDragging)
    }

    @Test func theSheetIsNotGrabbedAtTheHandlesCentreOrOffTheSheet() throws {
        let (r, tool) = try rig(azimuth: -.pi / 2 + 0.9)
        let c = pixel(r, tool.plane.position)
        #expect(tool.sheetPoint(x: c.x + 20, y: c.y, width: w, height: h) == nil, "within 80 points of the handle")
        r.camera.distance = 400                  // the 83 mm sheet is a patch in the middle of the view
        #expect(tool.sheetPoint(x: 2, y: 2, width: w, height: h) == nil, "off the sheet")
    }

    @Test func thePushIsHeldWithinThePartsReach() throws {
        let (r, tool) = try rig(azimuth: -.pi / 2 + 0.9)
        let c = tool.plane.position, n = tool.plane.normal
        let on = pixel(r, c + tool.plane.axisY * 35)
        #expect(tool.begin(x: on.x, y: on.y, width: w, height: h))
        let far = pixel(r, c + tool.plane.axisY * 35 + n * 400)
        tool.dragged(x: far.x, y: far.y, width: w, height: h, snap: true)
        let reach = simd_length(try #require(tool.partBox).size) / 2
        #expect(simd_dot(tool.plane.position - c, n) <= reach + 1e-3)
        tool.ended()
    }

    @Test func shiftScrollPushesOneMillimetreANotchDeeperAwayFromTheEye() throws {
        let (r, tool) = try rig(azimuth: -.pi / 2 + 0.9)
        let c = tool.plane.position, n = tool.plane.normal
        let away: Float = simd_dot(r.camera.basis.forward, n) >= 0 ? 1 : -1
        tool.scrollPush(120)                       // scrolling down: `-sign(d) * away`
        #expect(abs(simd_dot(tool.plane.position - c, n) - (-away)) < 1e-4)
        tool.scrollPush(-120)
        #expect(simd_length(tool.plane.position - c) < 1e-4)
    }

    @Test func thePartsRingsStepAsideWhileTheCutIsOn() throws {
        let c = try #require(ViewerController())
        let cube = GeneratedMesh.cube(edge: 40, device: c.renderer.device)
        c.show([cube], name: "cube")
        #expect(c.renderer.gizmo != nil)
        c.section.setOn(true)
        #expect(c.renderer.gizmo == nil)
        #expect(c.ringAt(x: 256, y: 256, width: 512, height: 512) == nil)
        c.section.setOn(false)
        #expect(c.renderer.gizmo != nil)
    }
}
