import Metal
import Testing
import simd
@testable import Viewer

/// The Move tool's pointer (`web/ui/move.js`): the height a model is grabbed at is that of whatever surface the
/// pointer is on -- its part, its fins or its pad.
@MainActor
@Suite("Move tool")
struct MoveToolTests {
    func rig() throws -> ViewerRenderer {
        let r = try #require(ViewerRenderer(), "no Metal device")
        let cube = GeneratedMesh.cube(edge: 40, device: r.device)
        r.setMesh(cube)
        r.setPose(rotation: matrix_identity_float3x3, offset: seatingOffset(bounds: cube.bounds, rotation: matrix_identity_float3x3))
        return r
    }

    func down(at x: Float, _ y: Float) -> (origin: SIMD3<Float>, direction: SIMD3<Float>) { (SIMD3(x, y, 200), SIMD3(0, 0, -1)) }

    /// Triangles as the engine gives them: position then normal, six floats a vertex.
    func layer(_ r: ViewerRenderer, fins: [[Float]], pad: [[Float]]) -> SupportLayer {
        func buffer(_ tris: [[Float]]) -> (MTLBuffer?, Int) {
            var flat: [Float] = []
            for t in tris { for k in 0..<3 { flat += [t[k * 3], t[k * 3 + 1], t[k * 3 + 2], 0, 0, 1] } }
            guard !flat.isEmpty else { return (nil, 0) }
            return (r.device.makeBuffer(bytes: flat, length: flat.count * 4, options: .storageModeShared), flat.count / 6)
        }
        let (f, fn) = buffer(fins), (p, pn) = buffer(pad)
        return SupportLayer(fins: f, finVertices: fn, pad: p, padVertices: pn)
    }

    @Test func theHeightIsThePartsTopWhereTheRayMeetsIt() throws {
        let r = try rig()
        #expect(abs((r.hitHeight(of: .live, ray: down(at: 0, 0)) ?? -1) - 40) < 1e-3)
        #expect(r.hitHeight(of: .live, ray: down(at: 100, 100)) == nil)
    }

    @Test func aFinOrThePadIsGrabbedAtItsOwnHeight() throws {
        let r = try rig()
        // a fin sheet standing 30 mm out from the cube, 15 mm tall; a pad sheet at 0.5 mm further out
        r.setSupports(layer(r, fins: [[30, -5, 0, 30, 5, 0, 30, 0, 15], [30, -5, 15, 30, 5, 15, 30, 5, 0]],
                            pad: [[-60, -60, 0.5, 60, -60, 0.5, 60, 60, 0.5]]))
        // the ray down at x = 30 meets the pad under the fin's edge -- at a horizontal fin there is none: the pad
        let onPad = r.hitHeight(of: .live, ray: down(at: 50, -40))
        #expect(abs((onPad ?? -1) - 0.5) < 1e-3, "\(String(describing: onPad))")
        // a ray across the fin sheet meets it at its height
        let side = (origin: SIMD3<Float>(100, 0, 8), direction: SIMD3<Float>(-1, 0, 0))
        let onFin = r.hitHeight(of: .live, ray: side)
        #expect(abs((onFin ?? -1) - 8) < 1e-3, "\(String(describing: onFin))")
        // the part still wins where it is nearest
        #expect(abs((r.hitHeight(of: .live, ray: down(at: 0, 0)) ?? -1) - 40) < 1e-3)
    }

    /// A hand-placed wall is the model's too (`modelAt` raycasts `drawnMesh`, `ui/move.js:76`): the live model's and a
    /// parked one's are grabbed at their own height, and a click on the live one's picks the live model.
    @Test func aDrawnWallIsGrabbedAtItsOwnHeight() async throws {
        let r = try rig()
        func mesh(_ tris: [[Float]]) throws -> OverlayMesh {
            var flat: [Float] = []
            for t in tris { for k in 0..<3 { flat += [t[k * 3], t[k * 3 + 1], t[k * 3 + 2], 0, 0, 1] } }
            let b = try #require(r.device.makeBuffer(bytes: flat, length: flat.count * 4, options: .storageModeShared))
            return OverlayMesh(buffer: b, vertices: flat.count / 6)
        }
        // a flat sheet 12 mm up, beside the cube
        let wall = try mesh([[40, -10, 12, 60, -10, 12, 60, 10, 12], [40, -10, 12, 60, 10, 12, 40, 10, 12]])
        var scene = DrawScene()
        scene.walls = wall
        r.setDrawScene(scene)
        let onWall = r.hitHeight(of: .live, ray: down(at: 50, 0))
        #expect(abs((onWall ?? -1) - 12) < 1e-3, "\(String(describing: onWall))")
        // the id pass: looking down at the wall, the live model
        r.camera = OrbitCamera(target: [50, 0, 0], distance: 200, azimuth: 0, elevation: OrbitCamera.maxElevation, aspect: 1)
        #expect(await r.pickModel(x: 128, y: 128, width: 256, height: 256) == .live)
        // a parked model's wall, about its own spot 300 mm along X
        let cube = GeneratedMesh.cube(edge: 40, device: r.device)
        let flags = try #require(r.device.makeBuffer(length: 64, options: .storageModeShared))
        r.setPlateScene(PlateScene(beds: [.zero], edited: 0, parked: [
            ParkedDrawable(mesh: cube, flags: flags, rotation: matrix_identity_float3x3,
                           offset: seatingOffset(bounds: cube.bounds, rotation: matrix_identity_float3x3), origin: SIMD2(300, 0), drawn: wall),
        ]))
        let parked = r.hitHeight(of: .parked(0), ray: down(at: 350, 0))
        #expect(abs((parked ?? -1) - 12) < 1e-3, "\(String(describing: parked))")
    }
}
