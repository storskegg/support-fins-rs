import Metal
import Testing
import simd
@testable import Viewer

@Suite("Support segments")
struct SupportSegmentTests {
    func fin(_ r: Range<Int>) -> SupportSegment { SupportSegment(range: r, kind: .fin) }
    func crest(_ r: Range<Int>) -> SupportSegment { SupportSegment(range: r, kind: .crest) }

    @Test func noMasksIsOneFinSegment() {
        #expect(SupportSegments.fins(total: 30, hidden: [], crest: []) == [fin(0..<30)])
        #expect(SupportSegments.fins(total: 0, hidden: [], crest: []) == [])
    }

    @Test func hiddenRangesAreCutOut() {
        #expect(SupportSegments.fins(total: 30, hidden: [9..<18], crest: []) == [fin(0..<9), fin(18..<30)])
        #expect(SupportSegments.fins(total: 30, hidden: [0..<9, 21..<30], crest: []) == [fin(9..<21)])
        #expect(SupportSegments.fins(total: 30, hidden: [0..<30], crest: []) == [])
    }

    @Test func crestRangesGetTheirOwnColour() {
        #expect(SupportSegments.fins(total: 30, hidden: [], crest: [6..<12, 24..<30])
            == [fin(0..<6), crest(6..<12), fin(12..<24), crest(24..<30)])
        // a hidden fin takes its crest with it
        #expect(SupportSegments.fins(total: 30, hidden: [6..<12], crest: [6..<12]) == [fin(0..<6), fin(12..<30)])
    }

    @Test func unsortedOverlappingAndOutOfRangeMasksAreNormalised() {
        #expect(SupportSegments.fins(total: 30, hidden: [18..<27, 9..<21, 100..<120, -5..<0, 4..<4], crest: [])
            == [fin(0..<9), fin(27..<30)])
        #expect(SupportSegments.fins(total: 30, hidden: [], crest: [24..<60, 3..<9, 6..<12])
            == [fin(0..<3), crest(3..<12), fin(12..<24), crest(24..<30)])
    }

    @Test func segmentsAreDisjointSortedAndCoverEveryVisibleVertex() {
        let hidden = [3..<6, 30..<33, 12..<15]
        let segs = SupportSegments.fins(total: 45, hidden: hidden, crest: [9..<18, 39..<45])
        for (a, b) in zip(segs, segs.dropFirst()) { #expect(a.range.upperBound <= b.range.lowerBound) }
        let drawn = Set(segs.flatMap { Array($0.range) })
        for v in 0..<45 { #expect(drawn.contains(v) == !hidden.contains { $0.contains(v) }, "vertex \(v)") }
    }
}

/// Fin, pad and crest colours by offscreen readback.
@MainActor
@Suite("Support rendering")
struct SupportRenderTests {
    let size = 256

    /// Two triangles of an axis-aligned square on z = 5 facing up, appended as position + normal.
    func quad(_ x: Float, _ y: Float, _ edge: Float, into data: inout [Float]) {
        let p: [SIMD3<Float>] = [[x, y, 5], [x + edge, y, 5], [x + edge, y + edge, 5], [x, y + edge, 5]]
        for i in [0, 1, 2, 0, 2, 3] { data += [p[i].x, p[i].y, p[i].z, 0, 0, 1] }
    }

    func layer(_ r: ViewerRenderer, finQuads: [(Float, Float)], padQuads: [(Float, Float)] = []) -> SupportLayer {
        var f: [Float] = [], p: [Float] = []
        for q in finQuads { quad(q.0, q.1, 15, into: &f) }
        for q in padQuads { quad(q.0, q.1, 15, into: &p) }
        return SupportLayer(
            fins: f.isEmpty ? nil : r.device.makeBuffer(bytes: f, length: f.count * 4, options: .storageModeShared),
            finVertices: f.count / 6,
            pad: p.isEmpty ? nil : r.device.makeBuffer(bytes: p, length: p.count * 4, options: .storageModeShared),
            padVertices: p.count / 6)
    }

    func render(_ r: ViewerRenderer) -> [UInt8] {
        r.camera = OrbitCamera(target: [30, 7.5, 5], distance: 120, azimuth: -.pi / 2, elevation: OrbitCamera.maxElevation, aspect: 1)
        return ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
    }

    func count(_ px: [UInt8], _ test: (Int, Int, Int) -> Bool) -> Int {
        var n = 0
        for i in stride(from: 0, to: px.count, by: 4) where test(Int(px[i + 2]), Int(px[i + 1]), Int(px[i])) { n += 1 }
        return n
    }

    // lit colours stay in the hue of the flat colour (web finMaterial / padMaterial / the crest)
    func isFin(_ r: Int, _ g: Int, _ b: Int) -> Bool { g > 120 && g > r + 40 && g > b + 30 }
    func isPad(_ r: Int, _ g: Int, _ b: Int) -> Bool { r > 150 && r > g + 15 && g > b + 50 }
    func isCrest(_ r: Int, _ g: Int, _ b: Int) -> Bool { b > 150 && b > g + 50 && r > g + 10 }
    // 0.55 of the red (1, 0.3, 0.3) over the lit green fin: a warm tan, (176, 144, 112) at this zoom; the amber pad has b < 90
    func isRed(_ r: Int, _ g: Int, _ b: Int) -> Bool { r > 150 && r > g + 20 && g > 110 && b > 90 && b < 140 && g < b + 55 }

    @Test func finsPadAndCrestKeepTheirColours() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        r.setSupports(layer(r, finQuads: [(0, 0), (20, 0)], padQuads: [(40, 0)]))
        r.setFinMask(hidden: [], crest: [6..<12])   // the second quad
        let px = render(r)
        let each = 15 * 15 * 0.6 // a quad's pixels at this zoom, roughly
        #expect(Double(count(px, isFin)) > each, "fin")
        #expect(Double(count(px, isCrest)) > each, "crest")
        #expect(Double(count(px, isPad)) > each, "pad")
        #expect(count(px, isRed) < 150, "edge blends only")
    }

    @Test func aHiddenFinIsNotDrawn() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        r.setSupports(layer(r, finQuads: [(0, 0), (20, 0)]))
        let both = count(render(r), isFin)
        r.setFinMask(hidden: [0..<6], crest: [])
        let one = count(render(r), isFin)
        r.setFinMask(hidden: [0..<12], crest: [])
        let none = count(render(r), isFin)
        #expect(both > 0 && one > both / 3 && one < both * 2 / 3, "\(both) -> \(one)")
        #expect(none == 0)
    }

    @Test func theHighlightedFinGoesRed() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        r.setSupports(layer(r, finQuads: [(0, 0), (20, 0)]))
        #expect(count(render(r), isRed) == 0)
        r.setFinHighlight([6..<12])
        let px = render(r)
        #expect(count(px, isRed) > 800, "the highlighted fin")
        #expect(count(px, isFin) > 100, "the other fin stays green")
        r.setFinHighlight([])
        #expect(count(render(r), isRed) == 0)
    }

    @Test func staleFinsArePalerWhileARebuildIsOut() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        r.setSupports(layer(r, finQuads: [(0, 0)]))
        let fresh = count(render(r), isFin)
        r.dimSupports = true
        let dim = render(r)
        #expect(fresh > 100 && count(dim, isFin) < fresh / 4, "the same fins, paler: they stay on screen")
        #expect(count(dim) { r, g, b in g > b && g > r && g > 60 } > 100, "still green-ish, just toward the background")
        r.dimSupports = false
        #expect(count(render(r), isFin) == fresh)
    }

    @Test func aNewBuildResetsTheMasks() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        r.setSupports(layer(r, finQuads: [(0, 0)]))
        r.setFinMask(hidden: [0..<6], crest: [6..<12])
        r.setFinHighlight([0..<6])
        r.setSupports(layer(r, finQuads: [(0, 0)]))
        #expect(r.hiddenFinRanges.isEmpty && r.crestFinRanges.isEmpty && r.highlightedFinRanges.isEmpty)
        r.setSupports(nil)
        #expect(count(render(r), isFin) == 0)
    }

    @Test func supportsDrawWithNoPartAndOverThePlate() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        #expect(r.meshes.isEmpty)
        r.setSupports(layer(r, finQuads: [(0, 0)]))
        let px = render(r)
        // the plate's grid is still there, and so are the fins
        #expect(count(px) { r, g, b in r > 40 && g > 44 && b > 52 && !(g > r + 40) } > 100)
        #expect(count(px, isFin) > 100)
    }
}

/// Picking a fin by the pixel (`ui/remove.js` pickFin): the triangle of the fin buffer under the pointer.
@MainActor
@Suite("Support picking")
struct SupportPickTests {
    let w = 256, h = 256

    /// Two quads, A at x 0...15 and B at x 20...35, y 0...15, z = 5; triangles 0, 1 are A's and 2, 3 are B's.
    func scene() throws -> ViewerRenderer {
        let r = try #require(ViewerRenderer(), "no Metal device")
        var f: [Float] = []
        for x in [Float(0), 20] {
            let p: [SIMD3<Float>] = [[x, 0, 5], [x + 15, 0, 5], [x + 15, 15, 5], [x, 15, 5]]
            for i in [0, 1, 2, 0, 2, 3] { f += [p[i].x, p[i].y, p[i].z, 0, 0, 1] }
        }
        let buffer = r.device.makeBuffer(bytes: f, length: f.count * 4, options: .storageModeShared)
        r.setSupports(SupportLayer(fins: buffer, finVertices: 12, pad: nil, padVertices: 0))
        r.camera = OrbitCamera(target: [20, 7.5, 5], distance: 120, azimuth: -.pi / 2, elevation: OrbitCamera.maxElevation, aspect: 1)
        return r
    }

    /// The pixel (origin top left) a print-space point lands on.
    func pixel(_ r: ViewerRenderer, _ p: SIMD3<Float>) -> (Int, Int) {
        let c = r.camera.viewProjection * SIMD4(p, 1)
        let n = SIMD2(c.x, c.y) / c.w
        return (Int((n.x * 0.5 + 0.5) * Float(w)), Int((1 - (n.y * 0.5 + 0.5)) * Float(h)))
    }

    @Test func thePointedAtTriangleIsReported() async throws {
        let r = try scene()
        // a point well inside each triangle of the two quads (the quad splits along its 0-2 diagonal)
        let cases: [(SIMD3<Float>, Int)] = [([11, 4, 5], 0), ([4, 11, 5], 1), ([31, 4, 5], 2), ([24, 11, 5], 3)]
        for (p, tri) in cases {
            let (x, y) = pixel(r, p)
            #expect(await r.pickSupport(x: x, y: y, width: w, height: h) == tri, "\(p)")
        }
    }

    @Test func emptySpaceAndOutsideTheViewPickNothing() async throws {
        let r = try scene()
        let (x, y) = pixel(r, [17.5, 7.5, 5]) // between the quads
        #expect(await r.pickSupport(x: x, y: y, width: w, height: h) == nil)
        #expect(await r.pickSupport(x: -1, y: 5, width: w, height: h) == nil)
        #expect(await r.pickSupport(x: 5, y: h, width: w, height: h) == nil)
        r.setSupports(nil)
        #expect(await r.pickSupport(x: 128, y: 128, width: w, height: h) == nil)
    }

    @Test func aRemovedFinCannotBePicked() async throws {
        let r = try scene()
        r.setFinMask(hidden: [0..<6], crest: [])
        let a = pixel(r, [11, 4, 5]), b = pixel(r, [31, 4, 5])
        #expect(await r.pickSupport(x: a.0, y: a.1, width: w, height: h) == nil)
        #expect(await r.pickSupport(x: b.0, y: b.1, width: w, height: h) == 2)
    }
}

@MainActor
@Suite("Remove-fins pointer mode")
struct SupportPointerTests {
    func rig() throws -> (ViewerController, SupportPickTests) {
        let c = try #require(ViewerController(), "no Metal device")
        let t = SupportPickTests()
        let r = try t.scene()
        // reuse the scene's supports and camera on the controller's own renderer
        c.renderer.setSupports(r.supports)
        c.renderer.camera = r.camera
        return (c, t)
    }

    @Test func hoverReportsTheFinTriangleAndTheCursorHint() async throws {
        let (c, t) = try rig()
        var seen: [Int?] = []
        c.onSupportHover = { seen.append($0) }
        c.supportPicking = true
        let (x, y) = t.pixel(c.renderer, [31, 4, 5])
        c.hover(x: x, y: y, width: t.w, height: t.h)
        while seen.isEmpty { await Task.yield() }
        #expect(seen.last == 2 && c.hoverSupportTriangle == 2 && c.hoveringSupport)
        #expect(c.hoverFace == nil, "the part's faces are not hovered in this mode")
        c.clearHover()
        #expect(seen.last! == nil && !c.hoveringSupport)
    }

    @Test func aClickReportsTheTriangleAndNothingWhenTheModeIsOff() async throws {
        let (c, t) = try rig()
        var clicks: [Int?] = []
        c.onSupportClick = { clicks.append($0) }
        let (x, y) = t.pixel(c.renderer, [4, 11, 5])
        c.click(x: x, y: y, width: t.w, height: t.h)
        try await Task.sleep(for: .milliseconds(100))
        #expect(clicks.isEmpty, "off: a click is not a pick")
        c.supportPicking = true
        c.click(x: x, y: y, width: t.w, height: t.h)
        while clicks.isEmpty { await Task.yield() }
        #expect(clicks == [1])
        let empty = t.pixel(c.renderer, [17.5, 7.5, 5])
        c.click(x: empty.0, y: empty.1, width: t.w, height: t.h)
        while clicks.count < 2 { await Task.yield() }
        #expect(clicks[1] == nil)
    }

    @Test func cancelIsTheModesOnlyWhileItIsArmed() throws {
        let (c, _) = try rig()
        var cancelled = 0
        c.onCancelArmed = { cancelled += 1 }
        #expect(!c.cancelArmed() && cancelled == 0, "not armed: Esc and a right click are the view's")
        c.supportPicking = true
        #expect(c.cancelArmed() && cancelled == 1)
    }

    @Test func leavingTheModeClearsTheHighlight() throws {
        let (c, _) = try rig()
        c.supportPicking = true
        c.renderer.setFinHighlight([0..<3])
        c.supportPicking = false
        #expect(c.renderer.highlightedFinRanges.isEmpty)
    }
}
