import Metal
import Testing
import simd
@testable import Viewer

/// The plates in the scene (`PlateScene.swift`): a parked model beside the live one in the ghost look, the beds of
/// the row, the clash cells, and which model a pixel lands on.
@MainActor
@Suite("Plate scene")
struct PlateSceneTests {
    let size = 256

    func rig() throws -> (ViewerRenderer, GeneratedMesh) {
        let r = try #require(ViewerRenderer(), "no Metal device")
        let cube = GeneratedMesh.cube(edge: 20, center: [0, 0, 10], device: r.device)
        r.setMesh(cube)
        r.setPose(rotation: matrix_identity_float3x3, offset: .zero)
        return (r, cube)
    }

    /// A parked cube of 20 mm at scene x = `x`, all faces plain (no flag bits).
    func parked(_ r: ViewerRenderer, at x: Float) -> ParkedDrawable {
        let mesh = GeneratedMesh.cube(edge: 20, center: [0, 0, 10], device: r.device)
        let flags = r.device.makeBuffer(length: mesh.faceCount, options: .storageModeShared)!
        memset(flags.contents(), 0, mesh.faceCount)
        return ParkedDrawable(mesh: mesh, flags: flags, rotation: matrix_identity_float3x3, offset: .zero, origin: [x, 0])
    }

    /// Looking straight down at the origin from 300 mm.
    func topDown(_ r: ViewerRenderer, target: SIMD3<Float> = .zero) {
        r.camera = OrbitCamera(target: target, distance: 300, azimuth: -.pi / 2, elevation: OrbitCamera.maxElevation, aspect: 1)
    }

    func pixels(_ r: ViewerRenderer) -> [UInt8] { ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size)) }

    func at(_ px: [UInt8], _ x: Int, _ y: Int) -> SIMD3<Float> {
        let o = (y * size + x) * 4
        return SIMD3(Float(px[o + 2]), Float(px[o + 1]), Float(px[o]))
    }

    /// The pixel of scene point (`x`, `y`) on z = 0 seen from `topDown`.
    func pixel(_ r: ViewerRenderer, _ x: Float, _ y: Float, z: Float = 0) -> (Int, Int) {
        let n = r.camera.project([x, y, z])
        return (Int((n.x + 1) / 2 * Float(size)), Int((1 - n.y) / 2 * Float(size)))
    }

    @Test func aParkedModelIsDrawnWhereItStandsInTheGhostLook() throws {
        let (r, _) = try rig()
        topDown(r)
        r.setPlateScene(PlateScene(beds: [.zero], edited: 0, parked: [parked(r, at: 60)]))
        let px = pixels(r)
        let (lx, ly) = pixel(r, 0, 0, z: 20), (px1, py1) = pixel(r, 60, 0, z: 20)
        let live = at(px, lx, ly), ghost = at(px, px1, py1)
        // the top face is plain in both; the parked one is a shade darker (ghostPart tint 0x9aa1ad)
        #expect(live.x > 100 && ghost.x > 60, "\(live) \(ghost)")
        #expect(ghost.x < live.x * 0.9 && ghost.y < live.y * 0.9 && ghost.z < live.z * 0.9, "live \(live), parked \(ghost)")
    }

    @Test func aBedIsDrawnForEveryPlate() throws {
        let (r, _) = try rig()
        r.setMesh(nil)
        topDown(r, target: [140, 0, 0])
        r.camera.distance = 500
        let bg = Palette.background * 255
        let lighter = { (px: [UInt8], x0: Int, x1: Int) -> Int in
            var n = 0
            for y in 0..<size { for x in x0..<x1 where self.at(px, x, y).x > bg.x + 15 { n += 1 } }
            return n
        }
        let one = pixels(r)
        r.setPlateScene(PlateScene(beds: [.zero, [280, 0]], edited: 0))
        let two = pixels(r)
        // the second bed's grid shows on the right half; the first is unchanged on the left
        #expect(lighter(two, size / 2, size) > lighter(one, size / 2, size) + 200)
    }

    @Test func clashCellsAreRedThroughTheModels() throws {
        let (r, _) = try rig()
        topDown(r)
        r.setPlateScene(PlateScene(beds: [.zero], edited: 0, clash: [[-5, -5], [-4, -5], [-5, -4], [-4, -4]]))
        let px = pixels(r)
        // the cells are under the live cube's top face (z 20): drawn through it
        let (cx, cy) = pixel(r, -4, -4, z: 20)
        let c = at(px, cx, cy)
        #expect(c.x > 150 && c.x > c.y + 60 && c.x > c.z + 60, "\(c)")
    }

    @Test func aPickLandsOnTheLiveOrAParkedModelOrNothing() async throws {
        let (r, _) = try rig()
        topDown(r)
        r.setPlateScene(PlateScene(beds: [.zero], edited: 0, parked: [parked(r, at: 60), parked(r, at: -60)]))
        let (lx, ly) = pixel(r, 0, 0, z: 20), (ax, ay) = pixel(r, 60, 0, z: 20), (bx, by) = pixel(r, -60, 0, z: 20)
        #expect(await r.pickModel(x: lx, y: ly, width: size, height: size) == .live)
        #expect(await r.pickModel(x: ax, y: ay, width: size, height: size) == .parked(0))
        #expect(await r.pickModel(x: bx, y: by, width: size, height: size) == .parked(1))
        #expect(await r.pickModel(x: 3, y: 3, width: size, height: size) == nil)
    }

    @Test func theGroundPointUnderAPixelIsInSceneCoordinates() throws {
        let (r, _) = try rig()
        topDown(r, target: [30, 20, 0])
        let (x, y) = pixel(r, 30, 20)
        let p = try #require(r.groundPoint(x: x, y: y, width: size, height: size))
        #expect(abs(p.x - 30) < 1 && abs(p.y - 20) < 1, "\(p)")
        // a ray above the horizon never reaches the plate
        r.camera = OrbitCamera(target: [0, 0, 100], distance: 100, azimuth: 0, elevation: -0.2, aspect: 1)
        #expect(r.groundPoint(x: size / 2, y: 0, width: size, height: size) == nil)
    }
}
