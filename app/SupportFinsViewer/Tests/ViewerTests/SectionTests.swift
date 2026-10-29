import Metal
import Testing
import simd
@testable import Viewer

/// The section plane's maths (`web/ui/section.js`) and its rendering: the cut half is clipped, Ghost draws it as
/// faint glass, Hide leaves it out, and a pick skips what the cut hides.
@MainActor
@Suite("Section view")
struct SectionTests {
    func near(_ a: SIMD3<Float>, _ b: SIMD3<Float>, _ tol: Float = 1e-5) -> Bool { simd_length(a - b) < tol }

    @Test func aimsAtThePartAxisNearestTheViewKeepingTheFarHalf() {
        let box = SectionBox(min: [-10, -10, 0], max: [10, 10, 40])
        // looking mostly along +Y: the plane is square to Y, its normal pointing away from the eye
        let p = SectionPlane.aimed(look: simd_normalize(SIMD3(0.2, 1, -0.1)), model: matrix_identity_float4x4, box: box)
        #expect(near(p.normal, [0, 1, 0]))
        #expect(near(p.position, [0, 0, 20]))
        // looking along -X: the axis is turned round so the far half is kept
        let q = SectionPlane.aimed(look: [-1, 0, 0], model: matrix_identity_float4x4, box: box)
        #expect(near(q.normal, [-1, 0, 0]))
        // a turned part: its own axes count, not the world's
        let turned = simd_float4x4(simd_quatf(angle: .pi / 2, axis: [0, 0, 1]))     // part X -> world Y
        let r = SectionPlane.aimed(look: [0, 1, 0], model: turned, box: box)
        #expect(near(r.normal, [0, 1, 0]))
    }

    @Test func flipKeepsThePlaneAndTheOtherSide() {
        var p = SectionPlane(position: [1, 2, 3], orientation: SectionPlane.rotation(from: [0, 0, 1], to: simd_normalize([1, 1, 0])))
        let n = p.normal, probe = p.position + n * 5
        #expect(p.distance(to: probe) > 4.99)
        p.flip()
        #expect(near(p.normal, -n))
        #expect(near(p.position, [1, 2, 3]))
        #expect(p.distance(to: probe) < -4.99)
        p.flip()
        #expect(near(p.normal, n, 1e-4))
    }

    @Test func pushMovesAlongTheNormalAndTheClipFollows() {
        var p = SectionPlane(position: [0, 0, 5], orientation: SectionPlane.rotation(from: [0, 0, 1], to: [1, 0, 0]))
        p.push(3)
        #expect(near(p.position, [3, 0, 5]))
        #expect(abs(p.distance(to: [3, 7, 9])) < 1e-5)
        #expect(p.distance(to: [4, 0, 0]) > 0.99)
        #expect(p.distance(to: [2, 0, 0]) < -0.99)
    }

    @Test func fromUnitVectorsHandlesOppositeVectors() {
        let q = SectionPlane.rotation(from: [0, 0, 1], to: [0, 0, -1])
        #expect(near(q.act([0, 0, 1]), [0, 0, -1]))
        let r = SectionPlane.rotation(from: [0, 0, 1], to: [0, 1, 0])
        #expect(near(r.act([0, 0, 1]), [0, 1, 0]))
    }

    @Test func theSheetPushIsHeldWithinThePartsReach() {
        let box = SectionBox(min: [-10, -10, 0], max: [10, 10, 20])
        let reach = simd_length(box.size) / 2
        let at = SIMD3<Float>(0, 0, 10)                       // the centre
        #expect(SectionPlane.clampedPush(1000, start: at, normal: [0, 1, 0], box: box) == reach)
        #expect(SectionPlane.clampedPush(-1000, start: at, normal: [0, 1, 0], box: box) == -reach)
        #expect(SectionPlane.clampedPush(2, start: at, normal: [0, 1, 0], box: box) == 2)
        // already 4 mm in: 4 mm less to go
        #expect(abs(SectionPlane.clampedPush(1000, start: [0, 4, 10], normal: [0, 1, 0], box: box) - (reach - 4)) < 1e-5)
    }

    @Test func theSheetIsTheBoxDiagonalAndAFifth() {
        let box = SectionBox(min: [0, 0, 0], max: [3, 4, 12])
        #expect(abs(SectionPlane.sheetSize(for: box) - 13 * 1.2) < 1e-5)
    }

    @Test func theMatrixRoundTrips() {
        let p = SectionPlane(position: [4, -5, 6], orientation: simd_quatf(angle: 0.7, axis: simd_normalize([1, 2, 3])))
        let q = SectionPlane(matrix: p.matrix)
        #expect(near(q.position, p.position))
        #expect(near(q.normal, p.normal, 1e-5) && near(q.axisX, p.axisX, 1e-5))
    }

    // MARK: rendering

    let size = 256

    /// Aim from +X (so the plane keeps x < 0), then look at the cube from the front: the left half stays.
    func rig(_ on: Bool, ghost: Bool = true) throws -> ViewerRenderer {
        let r = try #require(ViewerRenderer(), "no Metal device")
        let cube = GeneratedMesh.cube(edge: 40, device: r.device)
        r.setMesh(cube)
        r.setPose(rotation: matrix_identity_float3x3, offset: seatingOffset(bounds: cube.bounds, rotation: matrix_identity_float3x3))
        r.camera = OrbitCamera(target: [0, 0, 20], distance: 100, azimuth: 0, elevation: 0, aspect: 1)
        if on { r.section.setOn(true) }
        r.section.ghost = ghost
        r.camera = OrbitCamera(target: [0, 0, 20], distance: 100, azimuth: -.pi / 2, elevation: 0.3, aspect: 1)
        return r
    }

    func bright(_ px: [UInt8], columns: Range<Int>) -> Int {
        var n = 0
        for y in 75..<175 { for x in columns {
            let o = (y * size + x) * 4
            if px[o + 2] > 100 && px[o + 1] > 100 && px[o] > 150 { n += 1 }   // the cube's plain grey-blue, not the handle
        } }
        return n
    }

    /// Pixels that differ from the background by more than a hair.
    func different(_ px: [UInt8], columns: Range<Int>) -> Int {
        let bg = Palette.background * 255
        var n = 0
        for y in 75..<175 { for x in columns {
            let o = (y * size + x) * 4
            if abs(Float(px[o + 2]) - bg.x) > 4 || abs(Float(px[o + 1]) - bg.y) > 4 || abs(Float(px[o]) - bg.z) > 4 { n += 1 }
        } }
        return n
    }

    @Test func theCutHalfIsLeftOutWhenHidden() throws {
        let whole = ViewerRenderer.pixels(of: try rig(false).renderOffscreen(width: size, height: size))
        let r = try rig(true, ghost: false)
        #expect(r.section.isOn)
        #expect(SectionTool.nearly(r.section.plane.normal, [-1, 0, 0]))
        let cut = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        let right = 140..<185
        #expect(bright(whole, columns: right) > 500, "the whole cube has a right half")
        #expect(bright(cut, columns: right) == 0, "Hide leaves the cut half out: \(bright(cut, columns: right))")
        #expect(bright(cut, columns: 72..<115) > 500, "the kept half is there")
    }

    @Test func ghostDrawsTheCutHalfAsFaintGlass() throws {
        let hidden = ViewerRenderer.pixels(of: try rig(true, ghost: false).renderOffscreen(width: size, height: size))
        let ghost = ViewerRenderer.pixels(of: try rig(true, ghost: true).renderOffscreen(width: size, height: size))
        let right = 140..<185
        #expect(different(ghost, columns: right) > different(hidden, columns: right) + 500)
        #expect(bright(ghost, columns: right) == 0, "glass, not solid: \(bright(ghost, columns: right))")
    }

    @Test func aPickSkipsWhatTheCutHides() async throws {
        let whole = try rig(false)
        let cut = try rig(true, ghost: true)
        let at = (x: 165, y: 128)                                    // on the cube's right half
        let before = await whole.pickFace(x: at.x, y: at.y, width: size, height: size)
        let after = await cut.pickFace(x: at.x, y: at.y, width: size, height: size)
        #expect(before != nil)
        #expect(after == nil, "clicks go through the glass")
        let kept = await cut.pickFace(x: 90, y: 128, width: size, height: size)
        #expect(kept != nil)
    }

    @Test func theCutFollowsThePartWhenItTurns() throws {
        let r = try rig(true)
        let before = r.section.plane.normal
        let turn = simd_float3x3(simd_quatf(angle: .pi / 2, axis: [0, 0, 1]))
        r.setPose(rotation: turn, offset: r.offset)
        #expect(SectionTool.nearly(r.section.plane.normal, turn * before))
        r.setPose(rotation: matrix_identity_float3x3, offset: r.offset)
        #expect(SectionTool.nearly(r.section.plane.normal, before))
    }

    @Test func switchingOffClearsTheClip() throws {
        let r = try rig(true)
        #expect(r.clipVector != .zero)
        r.section.setOn(false)
        #expect(r.clipVector == .zero)
        #expect(!r.section.busy)
    }
}

extension SectionTool {
    static func nearly(_ a: SIMD3<Float>, _ b: SIMD3<Float>, _ tol: Float = 1e-4) -> Bool { simd_length(a - b) < tol }
}
