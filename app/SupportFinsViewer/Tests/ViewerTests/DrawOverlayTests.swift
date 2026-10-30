import Metal
import Testing
import simd
@testable import Viewer

/// Draw's layers by offscreen readback: the walls, the selection, the ghost, the band and the dots that keep their size
/// on screen (`web/ui/walls.js`, `web/ui/braceghost.js`).
@MainActor
@Suite("Draw overlay")
struct DrawOverlayTests {
    let size = 256

    /// A square on z = 5 facing up, as position + normal.
    func quad(_ r: ViewerRenderer, x: Float, y: Float, edge: Float = 15) -> OverlayMesh {
        let p: [SIMD3<Float>] = [[x, y, 5], [x + edge, y, 5], [x + edge, y + edge, 5], [x, y + edge, 5]]
        var data: [Float] = []
        for i in [0, 1, 2, 0, 2, 3] { data += [p[i].x, p[i].y, p[i].z, 0, 0, 1] }
        return OverlayMesh(buffer: r.device.makeBuffer(bytes: data, length: data.count * 4, options: .storageModeShared)!, vertices: 6)
    }

    func render(_ r: ViewerRenderer, distance: Float = 120) -> [UInt8] {
        r.camera = OrbitCamera(target: [7.5, 7.5, 5], distance: distance, azimuth: -.pi / 2, elevation: OrbitCamera.maxElevation, aspect: 1)
        return ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
    }

    func middle(_ px: [UInt8]) -> SIMD3<Float> {
        let o = (size / 2 * size + size / 2) * 4
        return SIMD3(Float(px[o + 2]), Float(px[o + 1]), Float(px[o]))
    }

    func isGreen(_ r: Int, _ g: Int, _ b: Int) -> Bool { g > 120 && g > r + 40 && g > b + 30 }
    func isAmber(_ r: Int, _ g: Int, _ b: Int) -> Bool { r > 150 && g > 90 && b < 110 && r - b > 80 }
    func isMint(_ r: Int, _ g: Int, _ b: Int) -> Bool { g > 200 && r < 160 && b < 200 }
    func isRedLine(_ r: Int, _ g: Int, _ b: Int) -> Bool { r > 200 && g < 160 && b < 160 }
    func isDot(_ r: Int, _ g: Int, _ b: Int) -> Bool { g > 150 && r < 160 }
    func isDotOnPart(_ r: Int, _ g: Int, _ b: Int) -> Bool { g > 150 && r < 160 && b < 160 }

    @Test func wallsAreTheFinGreenAndTheSelectionLiesAmberOverThem() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        var s = DrawScene()
        s.walls = quad(r, x: 0, y: 0)
        r.setDrawScene(s)
        let green = count(render(r), isGreen)
        #expect(green > 120, "the wall: \(green)")
        s.selection = quad(r, x: 0, y: 0)
        r.setDrawScene(s)
        let px = render(r)
        #expect(count(px, isAmber) > green / 2, "the selection over the wall")
        s.selection = nil
        r.setDrawScene(s)
        #expect(count(render(r), isAmber) == 0)
    }

    @Test func theStaleWallsAreAQuarterOfTheWayFromTheBackground() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        var s = DrawScene()
        s.walls = quad(r, x: -20, y: -20, edge: 60)
        r.setDrawScene(s)
        let opaque = middle(render(r))
        s.dim = true
        r.setDrawScene(s)
        let stale = middle(render(r))
        let bg = Palette.background * 255
        for k in 0..<3 {
            let want = bg[k] + 0.25 * (opaque[k] - bg[k])
            #expect(abs(stale[k] - want) < 4, "channel \(k): \(stale[k]) vs \(want)")
        }
    }

    @Test func theGhostIsTranslucentAndTheBraceGhostFollowsItsOpacity() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        var s = DrawScene()
        s.ghost = quad(r, x: -20, y: -20, edge: 60)
        r.setDrawScene(s)
        let ghost = middle(render(r))
        let bg = Palette.background * 255
        #expect(ghost != bg && simd_distance(ghost, bg) > 10, "something is drawn")
        s.ghost = nil
        s.braces = [(mesh: quad(r, x: -20, y: -20, edge: 60), opacity: 0.45)]
        r.setDrawScene(s)
        #expect(simd_distance(middle(render(r)), ghost) < 2, "a brace ghost at 0.45 looks like the wall ghost")
        s.braces = [(mesh: quad(r, x: -20, y: -20, edge: 60), opacity: 0.225)]
        r.setDrawScene(s)
        let faint = middle(render(r))
        #expect(simd_distance(faint, bg) < simd_distance(ghost, bg), "half the opacity is closer to the background")
        s.braces = [(mesh: quad(r, x: -20, y: -20, edge: 60), opacity: 0)]
        r.setDrawScene(s)
        #expect(simd_distance(middle(render(r)), bg) < 1, "a faded-out ghost is gone")
    }

    @Test func theBandIsGreenAndTurnsRedWhenTheWallCannotBuild() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        var s = DrawScene()
        s.band = [[-30, 7.5, 5], [40, 7.5, 5]]
        r.setDrawScene(s)
        let bandGreen = count(render(r), isMint)
        #expect(bandGreen > 100, "a line of mint: \(bandGreen)")
        s.bandBad = true
        r.setDrawScene(s)
        let bandRed = count(render(r), isRedLine)
        #expect(bandRed > 100, "a line of red: \(bandRed)")
    }

    @Test func aDotKeepsItsSizeOnScreenAtAnyZoom() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        var s = DrawScene()
        s.markers = [OverlayMarker(position: [7.5, 7.5, 5], radiusPixels: DrawPalette.dotPixels, color: DrawPalette.dot, alpha: 1)]
        r.setDrawScene(s)
        let near = count(render(r, distance: 60), isDot)
        let far = count(render(r, distance: 240), isDot)
        #expect(near > 40 && far > 40, "\(near) \(far)")
        #expect(Double(abs(near - far)) < Double(near) * 0.15, "the same size on screen: \(near) vs \(far)")
        s.markers = [OverlayMarker(position: [7.5, 7.5, 5], radiusPixels: 2 * DrawPalette.dotPixels, color: DrawPalette.dot, alpha: 1)]
        r.setDrawScene(s)
        let twice = count(render(r, distance: 60), isDot)
        #expect(Double(twice) > Double(near) * 3, "twice the radius is about four times the area: \(twice) vs \(near)")
    }

    @Test func aDotIsDrawnOverThePartLikeAHud() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        // a cube between the camera and the dot hides nothing of it
        r.setMesh(GeneratedMesh.cube(edge: 30, center: [7.5, 7.5, 12], device: r.device))
        var s = DrawScene()
        s.markers = [OverlayMarker(position: [7.5, 7.5, 5], radiusPixels: DrawPalette.dotPixels, color: DrawPalette.dot, alpha: 1)]
        r.setDrawScene(s)
        #expect(count(render(r), isDotOnPart) > 40)
    }

    @Test func nothingIsDrawnWithoutAScene() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        let before = render(r)
        var s = DrawScene()
        s.walls = quad(r, x: 0, y: 0)
        r.setDrawScene(s)
        #expect(render(r) != before)
        r.setDrawScene(nil)
        #expect(render(r) == before)
        #expect(r.drawScene == nil)
    }
}
