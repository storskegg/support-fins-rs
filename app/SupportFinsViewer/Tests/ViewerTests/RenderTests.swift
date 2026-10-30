import Metal
import Testing
import simd
@testable import Viewer

/// Offscreen render tests: draw into a texture and read the pixels back (no window, no display).
@MainActor
@Suite("Offscreen rendering")
struct RenderTests {
    let size = 256

    func rig() throws -> ViewerRenderer {
        try #require(ViewerRenderer(), "no Metal device")
    }

    func pixel(_ px: [UInt8], _ x: Int, _ y: Int) -> (r: Int, g: Int, b: Int) {
        let o = (y * size + x) * 4
        return (Int(px[o + 2]), Int(px[o + 1]), Int(px[o]))
    }

    func seat(_ r: ViewerRenderer, _ mesh: GeneratedMesh, _ degrees: SIMD3<Float>, threshold: Double = 45) async {
        let rot = rotationMatrix(degrees: degrees)
        r.setPose(rotation: rot, offset: seatingOffset(bounds: mesh.bounds, rotation: rot))
        r.setFlags(await mesh.analysis(thresholdDegrees: threshold, rotation: rot).flags)
    }

    @Test func emptySceneShowsThePlateOverTheBackground() throws {
        let r = try rig()
        r.camera = OrbitCamera(target: .zero, distance: 300, azimuth: 0, elevation: .pi / 2 - 0.3, aspect: 1)
        let px = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        let bg = Palette.background * 255
        // the corners are the background colour
        let c = pixel(px, 2, 2)
        #expect(abs(Float(c.r) - bg.x) < 2 && abs(Float(c.g) - bg.y) < 2 && abs(Float(c.b) - bg.z) < 2)
        // the grid is drawn: lines lighter than the background exist
        let lighter = count(px) { r, g, b in r > 40 && g > 44 && b > 52 }
        #expect(lighter > 200)
    }

    @Test func majorGridLineIsTheWebColour() throws {
        let r = try rig()
        // straight down the Z axis at the origin: the x = 0 and y = 0 lines cross at the middle
        r.camera = OrbitCamera(target: .zero, distance: 300, azimuth: 0, elevation: OrbitCamera.maxElevation, aspect: 1)
        let px = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        let major = Palette.gridMajor * 255
        var found = false
        for y in 125...131 { for x in 125...131 {
            let p = pixel(px, x, y)
            if abs(Float(p.r) - major.x) < 3 && abs(Float(p.g) - major.y) < 3 && abs(Float(p.b) - major.z) < 3 { found = true }
        } }
        #expect(found, "no major grid pixel at the middle")
    }

    @Test func overhangFacesAreRedFromBelow() async throws {
        let r = try rig()
        let cube = GeneratedMesh.cube(edge: 40, device: r.device)
        r.setMesh(cube)
        // 45 degrees about X: two lower faces lean 45 degrees from the plate, no more than 45: use 50
        await seat(r, cube, [45, 0, 0], threshold: 50)
        r.camera = OrbitCamera(target: [0, 0, 20], distance: 200, azimuth: -.pi / 2, elevation: -0.5, aspect: 1)
        let px = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        let red = count(px) { r, g, b in r > 120 && r > g + 55 && r > b + 55 }
        #expect(red > 500, "only \(red) red pixels")
    }

    @Test func aFaceOnTheBedIsBlueAndUprightFacesArePlain() async throws {
        let r = try rig()
        let cube = GeneratedMesh.cube(edge: 40, device: r.device)
        r.setMesh(cube)
        await seat(r, cube, [0, 0, 0])
        // from below the plate: the bottom face lies on z = 0
        r.camera = OrbitCamera(target: [0, 0, 20], distance: 200, azimuth: -.pi / 2, elevation: -1.2, aspect: 1)
        var px = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        let blue = count(px) { r, g, b in b > 90 && b > r + 40 }
        #expect(blue > 500, "only \(blue) bed-contact pixels")
        // from above: only plain faces, no red
        r.camera = OrbitCamera.home(distance: 200)
        r.camera.target = [0, 0, 20]
        px = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        #expect(count(px) { r, g, b in r > g + 55 && r > b + 55 && r > 150 } < 50)
        #expect(count(px) { r, g, b in abs(r - g) < 30 && abs(g - b) < 40 && r > 120 } > 500)
    }

    @Test func rotatingTheCubeChangesTheOverhangColoursLive() async throws {
        let r = try rig()
        let cube = GeneratedMesh.cube(edge: 40, device: r.device)
        r.setMesh(cube)
        r.camera = OrbitCamera(target: [0, 0, 20], distance: 200, azimuth: -.pi / 2, elevation: -0.5, aspect: 1)
        await seat(r, cube, [0, 0, 0])
        let flat = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        await seat(r, cube, [45, 0, 0], threshold: 50)
        let tilted = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        let isRed = { (r: Int, g: Int, b: Int) in r > 120 && r > g + 55 && r > b + 55 }
        #expect(count(flat, isRed) == 0)
        #expect(count(tilted, isRed) > 500)
    }

    @Test func smallOverhangsAreAmberAndHideWithTheDisplayToggle() async throws {
        let r = try rig()
        let cube = GeneratedMesh.cube(edge: 40, device: r.device)
        r.setMesh(cube)
        await seat(r, cube, [0, 0, 0])
        // all faces `over` but not `kept`: the sliver colour
        let flags = [UInt8](repeating: FaceFlags.over.rawValue, count: cube.faceCount)
        r.setFlags(r.device.makeBuffer(bytes: flags, length: flags.count, options: .storageModeShared)!)
        r.camera = OrbitCamera.home(distance: 200)
        r.camera.target = [0, 0, 20]
        let amber = { (r: Int, g: Int, b: Int) in r > 150 && g > 100 && b < g - 40 && r > g }
        let on = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        #expect(count(on, amber) > 500)
        r.highlightSmall = false
        let off = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        #expect(count(off, amber) < 50)
    }

    @Test func pickingFindsTheFaceUnderThePixel() async throws {
        let r = try rig()
        let cube = GeneratedMesh.cube(edge: 40, device: r.device)
        r.setMesh(cube)
        await seat(r, cube, [0, 0, 0])
        r.camera = OrbitCamera.home(distance: 200)
        r.camera.target = [0, 0, 20]
        let hit = try #require(await r.pickFace(x: size / 2, y: size / 2, width: size, height: size))
        // the home view sees the top (2, 3), front (4, 5) and right (8, 9) faces of the cube
        #expect([2, 3, 4, 5, 8, 9].contains(hit), "picked face \(hit)")
        // a corner of the view is empty
        #expect(await r.pickFace(x: 2, y: 2, width: size, height: size) == nil)
        // outside the view
        #expect(await r.pickFace(x: -1, y: 5, width: size, height: size) == nil)
    }

    @Test func pickedFaceMatchesWhatIsVisibleAndHoverTintsIt() async throws {
        let r = try rig()
        let cube = GeneratedMesh.cube(edge: 40, device: r.device)
        r.setMesh(cube)
        await seat(r, cube, [0, 0, 0])
        // straight down onto the top face (faces 2 and 3)
        r.camera = OrbitCamera(target: [0, 0, 20], distance: 200, azimuth: 0, elevation: OrbitCamera.maxElevation, aspect: 1)
        let hit = try #require(await r.pickFace(x: size / 2, y: size / 2, width: size, height: size))
        #expect(hit == 2 || hit == 3)
        let before = pixel(ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size)), size / 2, size / 2)
        r.setHover(hit)
        let after = pixel(ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size)), size / 2, size / 2)
        #expect(after.r < before.r - 20 && after.b >= before.b - 2, "hover did not tint the face: \(before) -> \(after)")
        r.setHover(nil)
    }

    @Test func theCameraCanBeSwungRoundWithoutTheMeshMoving() async throws {
        let r = try rig()
        let cube = GeneratedMesh.cube(edge: 40, device: r.device)
        r.setMesh(cube)
        await seat(r, cube, [0, 0, 0])
        r.camera = OrbitCamera.home(distance: 200)
        r.camera.target = [0, 0, 20]
        let a = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        r.camera.orbit(dx: 50, dy: 0, viewHeight: 400)
        let b = ViewerRenderer.pixels(of: r.renderOffscreen(width: size, height: size))
        #expect(a != b)
    }
}
