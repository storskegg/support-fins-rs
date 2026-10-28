import Metal
import Testing
import simd
@testable import Viewer

@MainActor
@Suite("ViewerController")
struct ControllerTests {
    func rig() throws -> (ViewerController, GeneratedMesh) {
        let c = try #require(ViewerController(), "no Metal device")
        let m = GeneratedMesh.cube(edge: 40, device: c.renderer.device)
        c.show(m, name: "cube")
        return (c, m)
    }

    /// Wait (briefly) for the flag task a pose change starts.
    func settle() async { for _ in 0..<20 { await Task.yield() } }

    func flags(_ c: ViewerController) -> [UInt8] {
        let n = c.faceCount
        return (0..<n).map { c.renderer.flagByte($0) }
    }

    @Test func showingAPartFramesItAndSeatsItOnThePlate() throws {
        let (c, _) = try rig()
        #expect(c.faceCount == 12 && c.meshName == "cube")
        #expect(abs(c.renderer.camera.distance - 40 * 2.1 * 1) < 1e-3 || c.renderer.camera.distance > 80)
        #expect(abs(c.renderer.offset.z - 20) < 1e-4)   // a 40 mm cube sits with its middle 20 mm up
        #expect(abs(c.renderer.camera.target.z - 20) < 1e-4)
    }

    @Test func rotatingRecomputesThePoseAndTheOverhangs() async throws {
        let (c, _) = try rig()
        await settle()
        let before = flags(c)
        #expect(before.filter { FaceFlags(rawValue: $0).contains(.kept) }.isEmpty)
        #expect(before.filter { FaceFlags(rawValue: $0).contains(.onBed) }.count == 2)   // the bottom's two triangles
        c.thresholdDegrees = 50
        c.rotation = rotationMatrix(degrees: [45, 0, 0])
        await settle()
        let after = flags(c)
        #expect(after.filter { FaceFlags(rawValue: $0).contains(.kept) }.count == 4)     // two faces lean 45 degrees
        #expect(c.renderer.rotation == rotationMatrix(degrees: [45, 0, 0]))
        // the cube is re-seated: its lowest edge is on the plate
        #expect(c.renderer.offset.z > 20)
    }

    @Test func aStaleFlagAnswerIsDropped() async throws {
        let (c, _) = try rig()
        c.rotation = rotationMatrix(degrees: [45, 0, 0])
        c.rotation = matrix_identity_float3x3
        await settle()
        #expect(flags(c).filter { FaceFlags(rawValue: $0).contains(.kept) }.isEmpty)
    }

    @Test func theVolumeReachesTheRenderer() throws {
        let (c, _) = try rig()
        c.volume = PrinterVolume.presets[4]
        #expect(c.renderer.volume == PrinterVolume(x: 300, y: 300, z: 300))
        c.volume = PrinterVolume(x: 0, y: 1, z: 1)           // invalid: ignored
        #expect(c.renderer.volume == PrinterVolume(x: 300, y: 300, z: 300))
    }

    @Test func gesturesMoveTheCamera() throws {
        let (c, _) = try rig()
        let a = c.renderer.camera
        c.orbit(dx: 20, dy: 5, viewHeight: 400)
        #expect(c.renderer.camera.azimuth != a.azimuth)
        c.pan(dx: 10, dy: 0, viewHeight: 400)
        #expect(c.renderer.camera.target != a.target)
        let d = c.renderer.camera.distance
        c.zoom(magnification: 0.5)
        #expect(c.renderer.camera.distance < d)
        c.apply(.front)
        #expect(simd_length(c.renderer.camera.direction - SIMD3(0, -1, 0)) < 1e-3)
    }

    @Test func hoverPicksAndClears() async throws {
        let (c, _) = try rig()
        c.renderer.camera = OrbitCamera(target: [0, 0, 20], distance: 200, azimuth: 0, elevation: OrbitCamera.maxElevation, aspect: 1)
        c.hover(x: 128, y: 128, width: 256, height: 256)
        for _ in 0..<200 where c.hoverFace == nil { try await Task.sleep(for: .milliseconds(5)) }
        #expect(c.hoverFace == 2 || c.hoverFace == 3)
        c.clearHover()
        #expect(c.hoverFace == nil && c.renderer.hoverFace == nil)
    }

    @Test func anIdleViewDrawsNothingNewUntilSomethingChanges() throws {
        let (c, _) = try rig()
        let n = c.renderer.changeCount
        c.renderer.setHover(nil)
        #expect(c.renderer.changeCount == n)
        c.renderer.camera.zoom(by: 1.1)
        #expect(c.renderer.changeCount > n)
    }

    @Test func severalPartsShareOnePoseAndAreNumberedAcrossTheFile() async throws {
        let c = try #require(ViewerController(), "no Metal device")
        let a = GeneratedMesh.cube(edge: 40, center: [-60, 0, 0], device: c.renderer.device)
        let b = GeneratedMesh.cube(edge: 40, center: [60, 0, 0], device: c.renderer.device)
        c.show([a, b], name: "pair")
        #expect(c.partCount == 2 && c.faceCount == 24)
        #expect(c.renderer.faceBases == [0, 12, 24])
        #expect(abs(c.size.x - 160) < 1e-3)                      // the union of both
        #expect(c.renderer.locate(13)! == (part: 1, face: 1))
        await settle()
        // straight down onto each cube: the left one is part 0, the right one part 1
        c.renderer.camera = OrbitCamera(target: [0, 0, 20], distance: 400, azimuth: 0, elevation: OrbitCamera.maxElevation, aspect: 1)
        let px = 256
        let left = c.renderer.camera.project([-60, 0, 40]), right = c.renderer.camera.project([60, 0, 40])
        let at = { (p: SIMD3<Float>) in (Int((p.x * 0.5 + 0.5) * Float(px)), Int((0.5 - p.y * 0.5) * Float(px))) }
        let l = try #require(await c.renderer.pickFace(x: at(left).0, y: at(left).1, width: px, height: px))
        let r = try #require(await c.renderer.pickFace(x: at(right).0, y: at(right).1, width: px, height: px))
        #expect(c.renderer.locate(l)?.part == 0 && c.renderer.locate(r)?.part == 1)
        #expect([2, 3].contains(c.renderer.locate(l)!.face) && [2, 3].contains(c.renderer.locate(r)!.face))
        c.hover(x: at(right).0, y: at(right).1, width: px, height: px)
        for _ in 0..<200 where c.hoverLocation == nil { try await Task.sleep(for: .milliseconds(5)) }
        #expect(c.hoverLocation?.part == 1)
    }
}
