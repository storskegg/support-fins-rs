import Foundation
import Metal
import SupportFinsCore
import Testing
import Viewer

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
    let path = try Scratch.file("build-model-tests-tshape.stl").path(percentEncoded: false)
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
    @Test func theSettingsOptionsAndTunablesBuildForReal() async throws {
        let model = try tShapeModel()
        func volume(_ edit: (inout FinSettings) -> Void) async throws -> Double {
            var s = FinSettings()
            edit(&s)
            model.build(rotation: identity, optionsJSON: s.optionsJSON(), tunablesJSON: s.tunablesJSON())
            await model.settled()
            #expect(model.error == nil, "the engine took the settings: \(model.error ?? "")")
            return try #require(model.summary).volume
        }
        let standard = try await volume { _ in }
        #expect(standard > 0)
        // a tunable the dialog cannot carry changes the build: the support gap and the wall lines thicken/lower the fins
        let gap = try await volume { $0.gap = 0.4 }
        let lines = try await volume { $0.wallLines = 4 }
        #expect(gap != standard, "propGap reaches the engine")
        #expect(lines != standard, "wallLines reaches the engine")
        let again = try await volume { _ in }
        #expect(again == standard, "the same settings build the same fins")
    }

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
        #expect(summary.boreWalls == 0, "a T shape stands in no bore, and the key is in the summary")
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

    @Test func aBuildReachesTheViewerWithNoCopyAndRemovalsMaskTheirRanges() async throws {
        let device = try #require(MTLCreateSystemDefaultDevice())
        let model = try tShapeModel()
        #expect(model.supportLayer(device: device) == nil && model.seatingOffset == nil)
        model.build(rotation: identity, optionsJSON: "{}")
        await model.settled()

        let layer = try #require(model.supportLayer(device: device))
        let result = try #require(model.latest)
        #expect(layer.finVertices == Int(result.fins().count()) && layer.finVertices > 0 && layer.stride == 6)
        // the fin buffer is the Rust allocation itself
        let fins = try #require(layer.fins)
        #expect(fins.contents() == UnsafeMutableRawPointer(bitPattern: UInt(result.fins().address())))
        #expect(layer.padVertices == Int(result.pad().count()))
        let o = try #require(model.summary).offset
        #expect(model.seatingOffset == SIMD3<Float>(Float(o.x), Float(o.y), Float(o.z)))

        // removing a fin hides exactly its ranges; an unknown key hides nothing
        let records = try #require(model.records)
        let key = try #require(model.keys.first?.sig)
        #expect(model.hiddenRanges(removed: []).isEmpty)
        #expect(model.hiddenRanges(removed: [key]) == model.ranges(ofFin: records.fins[0].id))
        #expect(model.hiddenRanges(removed: [FinKey(raw: "prop:900,900,5:6:0")]).isEmpty)
        let hidden = model.hiddenRanges(removed: [key])
        let drawn = SupportSegments.fins(total: layer.finVertices, hidden: hidden, crest: []).reduce(0) { $0 + $1.range.count }
        #expect(drawn == layer.finVertices - hidden.reduce(0) { $0 + $1.count })
        #expect(model.ranges(ofFin: -1).isEmpty)
    }

    @Test func showPutsTheBuildOnTheRendererWithTheRemovalsMasked() async throws {
        let renderer = try #require(ViewerRenderer(), "no Metal device")
        let model = try tShapeModel()
        model.show(on: renderer)
        #expect(renderer.supports == nil)
        model.build(rotation: identity, optionsJSON: "{}")
        await model.settled()
        let key = try #require(model.keys.first?.sig)
        model.show(on: renderer, removed: [key])
        #expect(renderer.supports?.finVertices == Int(model.latest!.fins().count()))
        #expect(renderer.hiddenFinRanges == model.hiddenRanges(removed: [key]) && !renderer.hiddenFinRanges.isEmpty)
        #expect(renderer.crestFinRanges.isEmpty)
        model.clear()
        model.show(on: renderer)
        #expect(renderer.supports == nil)
    }
}
