import Foundation
import SupportFinsCore
import Testing

@testable import Support_Fins

/// A binary STL of a T: a 10 x 10 stem under a 40 x 40 slab, so the slab's underside is an overhang the engine
/// puts fins under. Written to the app's temp directory (the test host is sandboxed, so the upstream checkout's
/// `tests/fixtures` cannot be read here) and loaded by the real `load_parts`.
private func tShapeSTL() throws -> String {
    func box(_ x0: Float, _ x1: Float, _ y0: Float, _ y1: Float, _ z0: Float, _ z1: Float) -> [[Float]] {
        let v: [[Float]] = [
            [x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0],
            [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1],
        ]
        let faces = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]]
        return faces.map { f in f.flatMap { v[$0] } }
    }
    let tris = box(-5, 5, -5, 5, 0, 20) + box(-20, 20, -20, 20, 20, 24)
    var data = Data(count: 80)
    var count = UInt32(tris.count)
    withUnsafeBytes(of: &count) { data.append(contentsOf: $0) }
    for t in tris {
        for f in [Float](repeating: 0, count: 3) + t { var x = f; withUnsafeBytes(of: &x) { data.append(contentsOf: $0) } }
        data.append(contentsOf: [0, 0])
    }
    let path = NSTemporaryDirectory() + "build-model-tests-tshape.stl"
    try data.write(to: URL(fileURLWithPath: path))
    return path
}

private let identity: [Double] = [1, 0, 0, 0, 1, 0, 0, 0, 1]

/// three.js `Euler(x, y, z, 'XYZ')` in degrees as nine column-major numbers (the CLI's `--rot`).
private func rotation(_ x: Double, _ y: Double, _ z: Double) -> [Double] {
    let r = { (d: Double) in d * Double.pi / 180 }
    let (cx, sx, cy, sy, cz, sz) = (cos(r(x)), sin(r(x)), cos(r(y)), sin(r(y)), cos(r(z)), sin(r(z)))
    return [
        cy * cz, cx * sz + sx * sy * cz, sx * sz - cx * sy * cz,
        -cy * sz, cx * cz - sx * sy * sz, sx * cz + cx * sy * sz,
        sy, -sx * cy, cx * cy,
    ]
}

@MainActor
private func tShapeModel() throws -> BuildModel {
    let part = try #require(try loadParts(path: try tShapeSTL()).first)
    return BuildModel(part: part)
}

@MainActor
@Suite("BuildModel")
struct BuildModelTests {
    @Test func aRealBuildIsDecodedIntoSummaryAndRecords() async throws {
        let model = try tShapeModel()
        #expect(model.latest == nil && !model.building)
        let g = model.build(rotation: identity, optionsJSON: "{}")
        #expect(g == 1 && model.building && model.generation == 1)
        await model.settled()
        #expect(!model.building && model.error == nil)

        let summary = try #require(model.summary)
        #expect(summary.volume > 0 && !summary.report.isEmpty && summary.stats.finTriangles > 0)
        #expect(summary.mode == "auto" && summary.offset.z.isFinite)
        let records = try #require(model.records)
        #expect(!records.fins.isEmpty)
        #expect(model.keys.count == records.fins.count)

        // ids are unique; every range lies inside the fin buffer, in whole triangles
        let result = try #require(model.latest)
        let vertices = Int(result.fins().count())
        #expect(Set(records.fins.map(\.id)).count == records.fins.count)
        for fin in records.fins {
            #expect(!fin.triRanges.isEmpty && fin.line.count >= 1)
            for r in fin.triRanges { #expect(r.from >= 0 && r.to <= vertices && r.from < r.to && r.count % 3 == 0) }
            #expect(["prop", "wedge", "sway"].contains(fin.kind))
        }
        // distinct fins never share a triangle
        let ranges = records.fins.flatMap(\.triRanges).sorted { $0.from < $1.from }
        for (a, b) in zip(ranges, ranges.dropFirst()) { #expect(a.to <= b.from) }
    }

    @Test func aNewerRequestSupersedesAnOlderOne() async throws {
        let model = try tShapeModel()
        let g1 = model.build(rotation: rotation(30, 0, 0), optionsJSON: "{}")
        let g2 = model.build(rotation: rotation(30, 0, 0), optionsJSON: #"{"mode":"full"}"#)
        #expect(g2 == g1 + 1)
        await model.settled()
        // whichever order the worker finishes in, only the newest request's build is shown
        #expect(model.summary?.mode == "full")
        #expect(model.generation == g2 && !model.building)
    }

    @Test func cancelDropsTheBuildAndClearForgetsTheFins() async throws {
        let model = try tShapeModel()
        model.build(rotation: identity, optionsJSON: "{}")
        model.cancel()
        #expect(!model.building)
        await model.settled() // returns at once
        try await Task.sleep(for: .milliseconds(500))
        #expect(model.latest == nil, "a cancelled build must not land")

        model.build(rotation: identity, optionsJSON: "{}")
        await model.settled()
        #expect(model.latest != nil)
        model.clear()
        #expect(model.latest == nil && model.summary == nil && model.records == nil && model.keys.isEmpty)
    }

    @Test func aFailedBuildKeepsTheFinsOnScreenAndReportsTheEngineMessage() async throws {
        let model = try tShapeModel()
        model.build(rotation: identity, optionsJSON: "{}")
        await model.settled()
        let before = try #require(model.summary)
        model.build(rotation: identity, optionsJSON: #"{"mode":"bogus"}"#)
        await model.settled()
        #expect(model.error?.isEmpty == false && !model.building)
        #expect(model.summary == before, "the last good build stays until a fresh one lands")
        // the next good build clears the error
        model.build(rotation: identity, optionsJSON: "{}")
        await model.settled()
        #expect(model.error == nil)
    }

    @Test func finKeysSurviveARebuildAndAnOptionsChange() async throws {
        let model = try tShapeModel()
        model.build(rotation: identity, optionsJSON: "{}")
        await model.settled()
        let first = model.keys.compactMap(\.sig)
        #expect(first.count == model.records?.fins.count)
        #expect(Set(first).count == first.count, "keys are unique within a build")

        // the same request again: the same keys
        model.build(rotation: identity, optionsJSON: "{}")
        await model.settled()
        #expect(model.keys.compactMap(\.sig) == first)

        // remove one fin, change a setting that moves nothing: the removal still finds its fin
        let gone = try #require(first.first)
        model.build(rotation: identity, optionsJSON: #"{"tineDensity":0.5}"#)
        await model.settled()
        let matched = model.matchedRemovals(of: [gone])
        #expect(matched.count == 1 && matched.values.first == gone)
        // a key from another pose matches nothing
        #expect(model.matchedRemovals(of: [FinKey(raw: "prop:500.0,500.0,5.0:6.0:0.5")]).isEmpty)
    }
}
