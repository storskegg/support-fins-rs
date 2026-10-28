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
