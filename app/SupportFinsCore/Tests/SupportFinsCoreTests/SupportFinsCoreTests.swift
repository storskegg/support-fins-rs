import Foundation
import Metal
import Testing

@testable import SupportFinsCore

// The upstream checkout's stress models (prototype/stress/models), found through the repo's UPSTREAM file.
private func modelPath(_ name: String) throws -> String {
    var url = URL(fileURLWithPath: #filePath)
    for _ in 0..<5 { url.deleteLastPathComponent() }  // Tests/SupportFinsCoreTests/<file> -> repo root
    let upstream = try String(contentsOf: url.appendingPathComponent("UPSTREAM"), encoding: .utf8)
    let line = try #require(upstream.split(separator: "\n").first { $0.hasPrefix("checkout=") })
    var root = String(line.dropFirst("checkout=".count)).trimmingCharacters(in: .whitespaces)
    if root.hasPrefix("~") { root = NSHomeDirectory() + root.dropFirst() }
    return root + "/prototype/stress/models/\(name).stl"
}

/// Binary STL parsed independently of Rust: per face, nine little-endian Float32 coordinates.
private func readBinarySTL(_ path: String) throws -> [Float] {
    let data = try Data(contentsOf: URL(fileURLWithPath: path))
    let count = Int(data.subdata(in: 80..<84).withUnsafeBytes { $0.loadUnaligned(as: UInt32.self) })
    var out: [Float] = []
    out.reserveCapacity(count * 9)
    for f in 0..<count {
        let base = 84 + f * 50 + 12
        for i in 0..<9 {
            let bits = data.subdata(in: (base + i * 4)..<(base + i * 4 + 4)).withUnsafeBytes {
                $0.loadUnaligned(as: UInt32.self)
            }
            out.append(Float(bitPattern: UInt32(littleEndian: bits)))
        }
    }
    return out
}

private func floats(_ buffer: MTLBuffer, count: Int) -> [Float] {
    Array(UnsafeBufferPointer(start: buffer.contents().assumingMemoryBound(to: Float.self), count: count))
}

private let device: MTLDevice = MTLCreateSystemDefaultDevice()!

@Test func engineVersionNamesTheUpstreamCommit() {
    #expect(engineVersion().contains("upstream 299ef49"))
}

@Test func loadsAStressModelThroughThePackage() throws {
    let parts = try loadParts(path: try modelPath("cube"))
    #expect(parts.count == 1)
    let info = parts[0].info()
    #expect(info.name == "cube")
    #expect(info.triangles == 12)
    #expect(info.watertight)
    #expect(info.size == [40, 40, 40])
}

@Test func loadErrorsSurfaceAsFfiError() {
    #expect(throws: FfiError.self) { try loadParts(path: "/no/such/file.stl") }
}

/// The zero-copy proof: the MTLBuffer is the Rust allocation, and its bytes are the mesh.
@Test(arguments: ["cube", "sphere", "lbracket"])
func meshMetalBufferIsTheRustAllocation(model: String) throws {
    let part = try #require(try loadParts(path: try modelPath(model)).first)
    let mesh = part.mesh()
    let buffer = try #require(mesh.metalBuffer(device: device))

    // same pages, not a copy
    #expect(buffer.contents() == UnsafeMutableRawPointer(bitPattern: UInt(mesh.address())))
    #expect(mesh.address() % 16384 == 0)
    #expect(buffer.length % 16384 == 0 && buffer.length >= Int(mesh.lenBytes()))
    #expect(mesh.stride() == 24)
    #expect(Int(mesh.count()) * 24 == Int(mesh.lenBytes()))

    // byte for byte the Rust-side floats: positions equal the STL's Float32s exactly (parsed here in
    // Swift), normals are the face normals repeated at each corner
    let soup = try readBinarySTL(try modelPath(model))
    let v = floats(buffer, count: Int(mesh.count()) * 6)
    #expect(Int(mesh.count()) == soup.count / 3)
    for i in 0..<Int(mesh.count()) {
        #expect(Array(v[i * 6..<i * 6 + 3]) == Array(soup[i * 3..<i * 3 + 3]), "vertex \(i)")
    }
    for f in 0..<Int(mesh.count()) / 3 {
        let a = soup[f * 9..<f * 9 + 3].map { $0 }, b = soup[f * 9 + 3..<f * 9 + 6].map { $0 }
        let c = soup[f * 9 + 6..<f * 9 + 9].map { $0 }
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]], w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]]
        var n = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]]
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).squareRoot()
        n = n.map { $0 / len }
        for corner in 0..<3 {
            let got = Array(v[(f * 3 + corner) * 6 + 3..<(f * 3 + corner) * 6 + 6])
            #expect(zip(got, n).allSatisfy { abs($0 - $1) < 1e-5 }, "normal of face \(f)")
        }
    }
}

@Test func theMetalBufferKeepsTheRustMemoryAlive() throws {
    var buffer: MTLBuffer?
    var expected: [Float] = []
    do {
        let part = try #require(try loadParts(path: try modelPath("sphere")).first)
        let mesh = part.mesh()
        buffer = mesh.metalBuffer(device: device)
        expected = floats(try #require(buffer), count: Int(mesh.count()) * 6)
    }  // the Swift references to the part and the GpuBuffer are gone; Metal's deallocator holds the buffer
    for _ in 0..<50 {  // churn the allocator: a freed page would be reused and the floats would change
        _ = (0..<20_000).map { Float($0) }
    }
    #expect(floats(try #require(buffer), count: expected.count) == expected)
    buffer = nil  // releasing the MTLBuffer runs the deallocator, freeing the Rust allocation
}

struct Pose: Sendable, CustomTestStringConvertible {
    var testDescription: String { model }
    let model: String
    let rot: [Double]
    let overArea: Double, bedArea: Double
    let overFaces: UInt64, regions: UInt64
    let offset: [Double], size: [Double]
    let over: Int, kept: Int, onBed: Int
}

// Reference values from sf_engine::overhangs::analyze (45 degrees) for these poses.
let poses: [Pose] = [
    Pose(model: "cube", rot: [1, 0, 0, 0, 1, 0, 0, -0.0, 1], overArea: 0, bedArea: 1600, overFaces: 0, regions: 0,
         offset: [-0.0, -0.0, 20], size: [40, 40, 40], over: 0, kept: 0, onBed: 2),
    Pose(model: "sphere", rot: [1.0, 0.0, 0.0, 0.0, 0.8660254037844387, 0.49999999999999994, 0.0, -0.49999999999999994, 0.8660254037844387],
         overArea: 1182.2910479507864, bedArea: 30.700003113463318, overFaces: 175, regions: 1,
         offset: [-0.0, -0.0, 25.988319585871455], size: [52, 51.97663917174291, 51.97663917174291], over: 175, kept: 175, onBed: 5),
    Pose(model: "lowledge", rot: [1.0, 0.0, 0.0, 0.0, 0.766044443118978, 0.6427876096865393, 0.0, -0.6427876096865393, 0.766044443118978],
         overArea: 2000, bedArea: 0, overFaces: 4, regions: 2,
         offset: [-0.0, -0.0, 13.775005562001404], size: [50, 32.18446804930782, 27.550011124002808], over: 4, kept: 4, onBed: 0),
]

@Test(arguments: poses)
func analyzeMatchesTheEngine(pose: Pose) throws {
    let part = try #require(try loadParts(path: try modelPath(pose.model)).first)
    let a = try part.analyze(thresholdDeg: 45, rot: pose.rot)
    #expect(a.overArea == pose.overArea)
    #expect(a.bedArea == pose.bedArea)
    #expect(a.overFaceCount == pose.overFaces)
    #expect(a.regionCount == pose.regions)
    #expect(a.offset == pose.offset)
    #expect(a.size == pose.size)

    // face flags: bit 0 over, bit 1 kept, bit 2 on the bed; read through Metal
    let flags = try part.faceFlags(thresholdDeg: 45, rot: pose.rot)
    let buffer = try #require(flags.metalBuffer(device: device))
    #expect(buffer.contents() == UnsafeMutableRawPointer(bitPattern: UInt(flags.address())))
    #expect(flags.stride() == 1 && Int(flags.count()) == Int(part.info().triangles))
    let bytes = Array(UnsafeBufferPointer(start: buffer.contents().assumingMemoryBound(to: UInt8.self), count: Int(flags.count())))
    #expect(bytes.filter { $0 & 1 != 0 }.count == pose.over)
    #expect(bytes.filter { $0 & 2 != 0 }.count == pose.kept)
    #expect(bytes.filter { $0 & 4 != 0 }.count == pose.onBed)
    #expect(bytes.allSatisfy { $0 >> 3 == 0 })
}

@Test func badPosesAndThresholdsAreEngineErrors() throws {
    let part = try #require(try loadParts(path: try modelPath("cube")).first)
    #expect(throws: FfiError.self) { try part.analyze(thresholdDeg: 45, rot: [1, 0, 0]) }
    #expect(throws: FfiError.self) { try part.faceFlags(thresholdDeg: .nan, rot: [1, 0, 0, 0, 1, 0, 0, 0, 1]) }
}
