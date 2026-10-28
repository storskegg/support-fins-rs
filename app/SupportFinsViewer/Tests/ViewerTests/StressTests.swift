import Metal
import Testing
import simd
@testable import Viewer

/// A part that records how it was analysed.
@MainActor
final class RecordingMesh: MeshSource {
    let inner: GeneratedMesh
    var summaries: [Bool] = []
    var vertexBuffer: MTLBuffer { inner.vertexBuffer }
    var vertexStride: Int { inner.vertexStride }
    var faceCount: Int { inner.faceCount }
    var bounds: (min: SIMD3<Float>, max: SIMD3<Float>) { inner.bounds }
    init(_ inner: GeneratedMesh) { self.inner = inner }
    func analysis(thresholdDegrees: Double, rotation: simd_float3x3) async -> MeshAnalysis {
        await analysis(thresholdDegrees: thresholdDegrees, rotation: rotation, wantsSummary: true)
    }
    func analysis(thresholdDegrees: Double, rotation: simd_float3x3, wantsSummary: Bool) async -> MeshAnalysis {
        summaries.append(wantsSummary)
        return await inner.analysis(thresholdDegrees: thresholdDegrees, rotation: rotation)
    }
}

/// The stress scene, the frame-time benchmark, and the per-frame paths the benchmark showed to matter.
@MainActor
@Suite("Stress scene and frame benchmark")
struct StressTests {
    func rig(triangles: Int = 60_000) throws -> ViewerRenderer {
        let r = try #require(ViewerRenderer(), "no Metal device")
        let mesh = StressMesh(triangles: triangles, device: r.device)
        r.setMesh(mesh)
        r.setPose(rotation: matrix_identity_float3x3, offset: [0, 0, 51.75])
        r.setFlags(StressScene.flags(of: mesh, thresholdDegrees: 45, rotation: matrix_identity_float3x3, device: r.device))
        r.setSupports(StressScene.supports(fins: 40, tines: 80, device: r.device))
        r.camera = OrbitCamera(target: [0, 0, 40], distance: 160, azimuth: 0.6, elevation: 0.5, aspect: 1.6)
        return r
    }

    func shot(_ r: ViewerRenderer) -> [UInt8] { ViewerRenderer.pixels(of: r.renderOffscreen(width: 320, height: 200)) }

    @Test func theStressMeshIsWoundOutward() throws {
        let r = try #require(ViewerRenderer(), "no Metal device")
        let mesh = StressMesh(triangles: 20_000, device: r.device)
        #expect(abs(mesh.faceCount - 20_000) < 1000)
        let p = mesh.vertexBuffer.contents().bindMemory(to: Float.self, capacity: mesh.faceCount * 18)
        var inward = 0
        for f in 0..<mesh.faceCount {
            let o = f * 18
            let c = (SIMD3(p[o], p[o + 1], p[o + 2]) + SIMD3(p[o + 6], p[o + 7], p[o + 8]) + SIMD3(p[o + 12], p[o + 13], p[o + 14])) / 3
            let n = SIMD3(p[o + 3], p[o + 4], p[o + 5])
            if simd_dot(n, c) < 0 { inward += 1 }
        }
        #expect(inward <= 2 * 140, "\(inward) faces point inward")     // the zero-area faces at the two poles have no normal

    }

    @Test func theBenchmarkRunsEveryScenarioAndLeavesTheSceneAsItFoundIt() throws {
        let r = try rig()
        let layers = (0..<2).map { StressScene.supports(fins: 30, tines: 60, device: r.device, seed: UInt64($0)) }
        let cam = r.camera, rot = r.rotation, off = r.offset
        for s in BenchScenario.allCases {
            let res = r.benchmark(frames: 24, warmup: 4, width: 320, height: 200, scenario: s, flagsEvery: 2, layers: layers)
            #expect(res.frames == 24 && res.triangles == r.totalFaces)
            #expect(res.gpu.mean > 0 && res.gpu.max >= res.gpu.p95 && res.gpu.p95 >= 0)
            #expect(res.cpu.mean > 0 && res.interval.mean > 0 && res.fps > 0)
            #expect(res.json.contains("\"scenario\": \"\(s.rawValue)\""))
        }
        #expect(r.camera == cam && r.rotation == rot && r.offset == off)
    }

    @Test func swappingTheFlagsAndTheBuildDoesNotChangeAPixel() throws {
        let r = try rig()
        let before = shot(r)
        // a new flags buffer with the same bytes (what an analysis hands over)
        let flags = r.flagBufferForTest(part: 0)
        let copy = r.device.makeBuffer(bytes: flags.contents(), length: flags.length, options: .storageModeShared)!
        r.setFlags(copy)
        #expect(shot(r) == before)
        // the build replaced by another and back: the residency set follows, the picture is the same
        let first = try #require(r.supports)
        r.setSupports(StressScene.supports(fins: 10, tines: 20, device: r.device, seed: 3))
        #expect(shot(r) != before)
        r.setSupports(first)
        #expect(shot(r) == before)
        r.setSupports(nil)
        r.setMeshes([])
        #expect(shot(r) != before)
    }

    @Test func aRingDragAsksForTheFlagsOnlyAndAReleaseForTheWholeAnalysis() async throws {
        let c = try #require(ViewerController(), "no Metal device")
        let rec = RecordingMesh(GeneratedMesh.cube(edge: 40, device: c.renderer.device))
        c.show(rec, name: "cube")
        c.renderer.camera = OrbitCamera(target: [0, 0, 20], distance: 220, azimuth: -.pi / 3, elevation: 0.5, aspect: 1)
        for _ in 0..<200 where rec.summaries.isEmpty { try await Task.sleep(for: .milliseconds(5)) }
        #expect(rec.summaries == [true], "the first analysis, not dragging")
        let size = 400
        let n = c.renderer.camera.project(c.gizmoCenter)
        let mid = (Int((n.x * 0.5 + 0.5) * Float(size)), Int((0.5 - n.y * 0.5) * Float(size)))
        #expect(c.beginRingDrag(x: mid.0 + 3, y: mid.1, width: size, height: size))
        c.dragRing(x: mid.0 + 60, y: mid.1 - 20, width: size, height: size, snap: false)
        for _ in 0..<200 where rec.summaries.count < 2 { try await Task.sleep(for: .milliseconds(5)) }
        #expect(rec.summaries.last == false, "a drag reads only the flags")
        c.endRingDrag()
        for _ in 0..<200 where rec.summaries.last != true { try await Task.sleep(for: .milliseconds(5)) }
        #expect(rec.summaries.last == true, "the release seats the part from the full analysis")
    }
}
