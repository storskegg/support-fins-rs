import Metal
import Testing
import simd
@testable import Viewer

/// The load arrow and the layer frames (`web/ui/strength.js`): their geometry, the Euler conversion a pose
/// takes to reach the view, and an offscreen render that reads the pixels back.
@MainActor
@Suite("Strength HUD")
struct HUDTests {
    @Test func layerFramesAreAboutEightMillimetresApartAndPokePastTheSilhouette() {
        // Math.max(3, Math.min(16, Math.round(size.z / 8) + 1))
        #expect(LayerFrames.count(sizeZ: 60) == 9)
        #expect(LayerFrames.count(sizeZ: 4) == 3)
        #expect(LayerFrames.count(sizeZ: 1000) == 16)
        let lines = LayerFrames.lines(size: SIMD3(40, 30, 60))
        #expect(lines.count == 9 * 4 * 2 * 3)
        let zs = Set(stride(from: 2, to: lines.count, by: 3).map { lines[$0] })
        #expect(zs.min() == 0 && zs.max() == 60 && zs.count == 9)
        // the margin: max(4, 0.12 * max(x, y)) = 4.8 past each side of 40 x 30
        let xs = stride(from: 0, to: lines.count, by: 3).map { lines[$0] }
        #expect(abs(xs.max()! - 24.8) < 1e-4 && abs(xs.min()! + 24.8) < 1e-4)
        let ys = stride(from: 1, to: lines.count, by: 3).map { lines[$0] }
        #expect(abs(ys.max()! - 19.8) < 1e-4)
        // a small part still gets a margin of 4
        let small = LayerFrames.lines(size: SIMD3(10, 10, 5))
        #expect(abs(small.max()! - 9) < 1e-4)
    }

    @Test func theArrowSpansThePartThroughItsCentre() {
        let center = SIMD3<Float>(5, -3, 20)
        for d in [SIMD3<Float>(0, 0, 1), SIMD3(1, 0, 0), simd_normalize(SIMD3<Float>(1, 1, 1))] {
            let v = LoadArrowGeometry.triangles(center: center, direction: d, extent: 40)
            #expect(v.count % 18 == 0 && !v.isEmpty)
            let len = LoadArrowGeometry.length(extent: 40)    // max(10, 40 * 0.6) = 24
            #expect(len == 24)
            let along = stride(from: 0, to: v.count, by: 6).map { simd_dot(SIMD3(v[$0], v[$0 + 1], v[$0 + 2]) - center, d) }
            #expect(abs(along.max()! - len / 2) < 1e-4, "the tip")
            #expect(abs(along.min()! + len / 2) < 1e-4, "the tail")
            // the head is at most min(len * 0.28, 12) long and min(len * 0.18, 7) wide
            let wide = stride(from: 0, to: v.count, by: 6).map { i -> Float in
                let p = SIMD3(v[i], v[i + 1], v[i + 2]) - center
                return simd_length(p - simd_dot(p, d) * d)
            }
            #expect(abs(wide.max()! - min(len * 0.18, 7) / 2) < 1e-4)
        }
        // a small part: the arrow is never shorter than 10 mm
        #expect(LoadArrowGeometry.length(extent: 5) == 10)
    }

    // MARK: offscreen pixels

    let size = 256
    func rig() throws -> ViewerRenderer {
        let r = try #require(ViewerRenderer(), "no Metal device")
        r.camera = OrbitCamera(target: SIMD3(0, 0, 10), distance: 120, azimuth: 0, elevation: OrbitCamera.maxElevation, aspect: 1)
        return r
    }
    func count(_ px: [UInt8], _ near: SIMD3<Float>, tolerance: Float = 24) -> Int {
        let want = near * 255
        var n = 0
        for i in stride(from: 0, to: px.count, by: 4)
        where abs(Float(px[i + 2]) - want.x) < tolerance && abs(Float(px[i + 1]) - want.y) < tolerance && abs(Float(px[i]) - want.z) < tolerance {
            n += 1
        }
        return n
    }

    /// The columns and rows the arrow's pixels reach.
    func extent(_ px: [UInt8]) -> (cols: Int, rows: Int) {
        var rows = Set<Int>(), cols = Set<Int>()
        let want = Palette.loadArrow * 255
        for y in 0..<size { for x in 0..<size {
            let o = (y * size + x) * 4
            if abs(Float(px[o + 2]) - want.x) < 24 && abs(Float(px[o + 1]) - want.y) < 24 && abs(Float(px[o]) - want.z) < 24 {
                rows.insert(y); cols.insert(x)
            }
        } }
        return (cols.count, rows.count)
    }

    @Test func theLoadArrowDrawsInItsColourAndIsGoneWhenCleared() throws {
        let r = try rig()
        let empty = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        #expect(count(empty, Palette.loadArrow) == 0)
        r.setLoadArrow(center: SIMD3(0, 0, 10), direction: SIMD3(1, 0, 0), extent: 40)
        #expect(r.hasLoadArrow)
        let drawn = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        #expect(count(drawn, Palette.loadArrow) > 150, "the arrow's pixels")
        let e = extent(drawn)
        #expect(max(e.cols, e.rows) > 2 * min(e.cols, e.rows), "a long thin arrow: \(e)")
        r.clearLoadArrow()
        #expect(count(ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size)), Palette.loadArrow) == 0)
    }

    @Test func theArrowTurnsWithThePart() throws {
        let r = try rig()
        r.setLoadArrow(center: SIMD3(0, 0, 0), direction: SIMD3(1, 0, 0), extent: 40)
        r.setPose(rotation: matrix_identity_float3x3, offset: SIMD3(0, 0, 10))
        let before = extent(ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size)))
        r.setPose(rotation: rotationMatrix(degrees: SIMD3(0, 0, 90)), offset: SIMD3(0, 0, 10))
        let after = extent(ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size)))
        #expect((before.cols > before.rows) != (after.cols > after.rows), "turned 90 degrees about Z, the arrow's long side swaps: \(before) \(after)")
        #expect(max(after.cols, after.rows) > 2 * min(after.cols, after.rows))
    }

    @Test func theLayerFramesDrawOverTheBackgroundAndGoWhenCleared() throws {
        let r = try rig()
        r.camera = OrbitCamera(target: SIMD3(0, 0, 30), distance: 200, azimuth: 0, elevation: 0.5, aspect: 1)
        let before = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        r.setLayerFrames(size: SIMD3(40, 40, 60))
        #expect(r.hasLayerFrames)
        let after = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        let changed = zip(before, after).filter { $0 != $1 }.count
        #expect(changed > 400, "the frames add pixels to the view: \(changed)")
        // a frame line is the layers colour blended at 0.4 over what is behind it: bluer than the background
        let blue = stride(from: 0, to: after.count, by: 4).filter { Int(after[$0]) - Int(before[$0]) > 20 }.count
        #expect(blue > 100)
        r.clearLayerFrames()
        #expect(ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size)) == before)
    }
}
