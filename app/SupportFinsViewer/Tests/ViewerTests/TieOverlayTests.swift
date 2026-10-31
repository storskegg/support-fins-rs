import Metal
import Testing
import simd
@testable import Viewer

/// Plate ties over the plate (`TieOverlay.swift`, issue #50): the links drawn dashed about the edited bed, and which
/// link a click lands on.
@MainActor
@Suite("Tie overlay")
struct TieOverlayTests {
    let size = 256

    func rig() throws -> ViewerRenderer {
        let r = try #require(ViewerRenderer(), "no Metal device")
        r.camera = OrbitCamera(target: .zero, distance: 300, azimuth: -.pi / 2, elevation: OrbitCamera.maxElevation, aspect: 1)
        return r
    }

    func pixel(_ r: ViewerRenderer, _ p: SIMD3<Float>) -> (Int, Int) {
        let n = r.camera.project(p)
        return (Int((n.x + 1) / 2 * Float(size)), Int((1 - n.y) / 2 * Float(size)))
    }

    @Test func aLinkIsCutIntoDashesThatEndAtItsEnd() {
        let d = ViewerRenderer.dashes(from: [0, 0, 5], to: [4, 0, 5], dash: 1, space: 0.5)
        #expect(d.count == 3)
        #expect(d.first?.0 == SIMD3(0, 0, 5) && d.first?.1 == SIMD3(1, 0, 5))
        #expect(d.last?.1 == SIMD3(4, 0, 5), "the last dash is cut at the link's end")
        #expect(ViewerRenderer.dashes(from: [1, 1, 1], to: [1, 1, 1]).isEmpty, "a point has no dashes")
    }

    @Test func aClickNearALinkFindsItAboutTheEditedBed() throws {
        let r = try rig()
        let links = [TieLinkLine(from: [-40, 0, 0], to: [-20, 0, 0], enabled: true, manual: false),
                     TieLinkLine(from: [20, 0, 0], to: [40, 0, 0], enabled: false, manual: false)]
        r.setTieScene(TieScene(links: links))
        let (x0, y0) = pixel(r, [-30, 0, 0]), (x1, y1) = pixel(r, [30, 0, 0])
        #expect(r.linkAt(x: x0, y: y0, width: size, height: size) == 0)
        #expect(r.linkAt(x: x1, y: y1 + 2, width: size, height: size) == 1)
        let (bx, by) = pixel(r, [0, 40, 0])
        #expect(r.linkAt(x: bx, y: by, width: size, height: size) == nil, "nothing near")
        // the edited bed is shifted (another model is live): the links move with it
        r.setPlateScene(PlateScene(beds: [[10, 0]], edited: 0))
        let (sx, sy) = pixel(r, [-20, 0, 0])
        #expect(r.linkAt(x: sx, y: sy, width: size, height: size) == 0)
        #expect(r.linkAt(x: x0 - 30, y: y0, width: size, height: size) == nil)
    }

    @Test func linksAndTiesDrawAndGoAway() throws {
        let r = try rig()
        let before = r.changeCount
        r.setTieScene(TieScene(links: [TieLinkLine(from: [-40, 0, 1], to: [40, 0, 1], enabled: true, manual: false)]))
        #expect(r.changeCount == before + 1)
        let px = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        let (x, y) = pixel(r, [0.6, 0, 1])
        let o = (y * size + x) * 4
        // the link colour (0x4da3ff): blue over the bed
        #expect(px[o] > 150 && px[o] > px[o + 2], "b \(px[o]) r \(px[o + 2])")
        r.setTieScene(nil)
        #expect(r.tieScene == nil)
        let n = r.changeCount
        r.setTieScene(nil)
        #expect(r.changeCount == n, "nothing to take off: no frame")
    }
}
