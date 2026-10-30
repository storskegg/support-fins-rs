import Foundation
import Metal
import Testing

@testable import SupportFinsCore

private func modelPath(_ name: String) throws -> String {
    var url = URL(fileURLWithPath: #filePath)
    for _ in 0..<5 { url.deleteLastPathComponent() }
    let upstream = try String(contentsOf: url.appendingPathComponent("UPSTREAM"), encoding: .utf8)
    let line = try #require(upstream.split(separator: "\n").first { $0.hasPrefix("checkout=") })
    var root = String(line.dropFirst("checkout=".count)).trimmingCharacters(in: .whitespaces)
    if root.hasPrefix("~") { root = NSHomeDirectory() + root.dropFirst() }
    return root + "/prototype/stress/models/\(name).stl"
}

/// three.js `Euler(x, y, z, 'XYZ')` as nine column-major numbers, the CLI's `--rot`.
private func rotation(_ x: Double, _ y: Double, _ z: Double) -> [Double] {
    let r = { (d: Double) in d * Double.pi / 180 }
    let (cx, sx, cy, sy, cz, sz) = (cos(r(x)), sin(r(x)), cos(r(y)), sin(r(y)), cos(r(z)), sin(r(z)))
    return [
        cy * cz, cx * sz + sx * sy * cz, sx * sz - cx * sy * cz,
        -cy * sz, cx * cz - sx * sy * sz, sx * cz + cx * sy * sz,
        sy, -sx * cy, cx * cy,
    ]
}

/// A listener that turns the worker thread's callbacks into an async stream.
private final class Collector: BuildListener, @unchecked Sendable {
    enum Event: Sendable {
        case done(UInt64, BuildResult)
        case failed(UInt64, FfiError)
    }
    let stream: AsyncStream<Event>
    private let continuation: AsyncStream<Event>.Continuation
    init() { (stream, continuation) = AsyncStream.makeStream() }
    func done(generation: UInt64, result: BuildResult) { continuation.yield(.done(generation, result)) }
    func failed(generation: UInt64, err: FfiError) { continuation.yield(.failed(generation, err)) }
    func finish() { continuation.finish() }
}

private let device: MTLDevice = MTLCreateSystemDefaultDevice()!

@Test func anAsyncBuildRoundTripsOnAStressModel() async throws {
    let part = try #require(try loadParts(path: try modelPath("lbracket")).first)
    let builder = Builder(part: part)
    let collector = Collector()
    let generation = try builder.build(rot: rotation(35, 0, 0), optsJson: "{}", tunablesJson: "{}", listener: collector)
    #expect(generation == 1)

    var delivered: BuildResult?
    for await event in collector.stream {
        guard case .done(let g, let result) = event else { Issue.record("the build failed"); return }
        #expect(g == 1)
        delivered = result
        break
    }
    let result = try #require(delivered)

    // the fins reach Metal as the Rust allocation, in print space (z up from the plate)
    let fins = result.fins()
    #expect(fins.stride() == 24 && fins.count() > 0 && fins.count() % 3 == 0)
    let buffer = try #require(fins.metalBuffer(device: device))
    #expect(buffer.contents() == UnsafeMutableRawPointer(bitPattern: UInt(fins.address())))
    let floats = UnsafeBufferPointer(start: buffer.contents().assumingMemoryBound(to: Float.self), count: Int(fins.count()) * 6)
    #expect(floats.enumerated().allSatisfy { $0.offset % 6 >= 3 || $0.element.isFinite })

    // the readout and the records are JSON
    let summary = try #require(
        try JSONSerialization.jsonObject(with: Data(result.summaryJson().utf8)) as? [String: Any])
    #expect((summary["tines"] as? Double ?? 0) > 0)
    #expect(!((summary["report"] as? String) ?? "").isEmpty)
    let records = try #require(
        try JSONSerialization.jsonObject(with: Data(result.recordsJson().utf8)) as? [String: Any])
    #expect(!((records["fins"] as? [Any]) ?? []).isEmpty)
    #expect(result.pad().stride() == 24)
}

@Test func aStaleGenerationIsNeverDelivered() async throws {
    let part = try #require(try loadParts(path: try modelPath("sphere")).first)
    let builder = Builder(part: part)
    let first = Collector(), second = Collector()
    let g1 = try builder.build(rot: rotation(30, 0, 0), optsJson: "{}", tunablesJson: "{}", listener: first)
    let g2 = try builder.build(rot: rotation(30, 0, 0), optsJson: #"{"mode":"full"}"#, tunablesJson: "{}", listener: second)
    #expect(g2 == g1 + 1)

    var latest: UInt64?
    for await event in second.stream {
        if case .done(let g, _) = event { latest = g }
        break
    }
    #expect(latest == g2)

    // the superseded build never shows up: give it time to (wrongly) arrive, then close its stream
    try await Task.sleep(for: .milliseconds(300))
    first.finish()
    var staleEvents = 0
    for await _ in first.stream { staleEvents += 1 }
    #expect(staleEvents == 0)
}

@Test func cancelDropsTheBuildInFlightAndALaterBuildStillArrives() async throws {
    let part = try #require(try loadParts(path: try modelPath("torus")).first)
    let builder = Builder(part: part)
    let dropped = Collector(), kept = Collector()
    _ = try builder.build(rot: rotation(45, 0, 0), optsJson: "{}", tunablesJson: "{}", listener: dropped)
    builder.cancel()
    let g = try builder.build(rot: rotation(45, 0, 0), optsJson: "{}", tunablesJson: "{}", listener: kept)
    var arrived: UInt64?
    for await event in kept.stream {
        if case .done(let gen, _) = event { arrived = gen }
        break
    }
    #expect(arrived == g)
    try await Task.sleep(for: .milliseconds(200))
    dropped.finish()
    var events = 0
    for await _ in dropped.stream { events += 1 }
    #expect(events == 0)
}

@Test func buildErrorsArriveAsFailures() async throws {
    let part = try #require(try loadParts(path: try modelPath("cube")).first)
    let builder = Builder(part: part)
    let collector = Collector()
    let g = try builder.build(rot: rotation(0, 0, 0), optsJson: #"{"mode":"bogus"}"#, tunablesJson: "{}", listener: collector)
    for await event in collector.stream {
        guard case .failed(let fg, let error) = event else { Issue.record("expected a failure"); return }
        #expect(fg == g)
        guard case .Engine = error else { Issue.record("expected an engine error"); return }
        break
    }
}

@Test func exportWritesAFileAndRefusesWhatItCannotDo() async throws {
    let part = try #require(try loadParts(path: try modelPath("sphere")).first)
    let builder = Builder(part: part)
    let collector = Collector()
    _ = try builder.build(rot: rotation(0, 0, 0), optsJson: "{}", tunablesJson: "{}", listener: collector)
    var built: BuildResult?
    for await event in collector.stream {
        if case .done(_, let r) = event { built = r }
        break
    }
    let result = try #require(built)
    let path = NSTemporaryDirectory() + "support-fins-core-test-\(ProcessInfo.processInfo.processIdentifier).stl"
    defer { try? FileManager.default.removeItem(atPath: path) }
    try export(path: path, parts: [part], builds: [result], optionsJson: "{}")
    let size = try #require(try FileManager.default.attributesOfItem(atPath: path)[.size] as? Int)
    // a binary STL: 80-byte header, a count, 50 bytes a triangle -- the part's plus the fins'
    #expect(size == 84 + 50 * (Int(part.info().triangles) + Int(result.fins().count() + result.pad().count()) / 3))
    #expect(throws: FfiError.self) { try export(path: path + ".obj", parts: [part], builds: [result], optionsJson: "{}") }
}

@Test func thePagesTunablesReachTheBuild() async throws {
    let part = try #require(try loadParts(path: try modelPath("sphere")).first)
    let rot = rotation(30, 0, 0)
    func fins(optsJson: String, tunablesJson: String) async throws -> (UInt32, Data) {
        let collector = Collector()
        let builder = Builder(part: part)  // dropped = cancelled (C-04): keep it until the result lands
    defer { withExtendedLifetime(builder) {} }
    _ = try builder.build(rot: rot, optsJson: optsJson, tunablesJson: tunablesJson, listener: collector)
        for await event in collector.stream {
            guard case .done(_, let result) = event else { Issue.record("the build failed"); break }
            let fins = result.fins()
            let bytes = UnsafeRawBufferPointer(
                start: UnsafeRawPointer(bitPattern: UInt(fins.address())), count: Int(fins.lenBytes()))
            return (fins.count(), Data(bytes))
        }
        throw CancellationError()
    }
    let plain = try await fins(optsJson: "{}", tunablesJson: "{}")
    let tuned = try await fins(optsJson: "{}", tunablesJson: #"{"wallLines":4,"roundFeet":true,"tallRibsHeight":40}"#)
    #expect(plain.1 != tuned.1, "wall lines and rounded feet change the supports")
    // the request-level extras ride in the options
    let noRibs = try await fins(optsJson: #"{"tallRibs":false}"#, tunablesJson: "{}")
    #expect(noRibs.0 > 0)
}
