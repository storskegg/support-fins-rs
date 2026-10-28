import Foundation
import SupportFinsCore
import Testing
import Viewer

@testable import Support_Fins

/// The M4b loop headless, through the pieces the window is made of (`ViewerModel` -> `PartSession` ->
/// `BuildWiring`/`BuildController` -> `BuildModel` -> readout -> `ExportModel`): open a part, turn Add fins on,
/// wait for the build, read the Fins row, change a setting and wait for the rebuild, export an STL. The UI test
/// (`SupportFinsUITests`) walks the same loop through the real window; this one runs where UI tests can't.
@MainActor
@Suite("End to end")
struct EndToEndTests {
    /// Waits (a few seconds at most) for `condition`.
    private func eventually(_ what: String, _ condition: () -> Bool) async throws {
        for _ in 0..<1000 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        Issue.record("timed out: \(what)")
    }

    @Test func openBuildReadRebuildAndExport() async throws {
        // open a part through the window's model and the engine's loader
        let model = ViewerModel()
        try #require(model.controller != nil, "no Metal device")
        let stl = URL(fileURLWithPath: try TestParts.write(
            TestParts.box(-5, 5, -5, 5, 0, 20) + TestParts.box(-30, 30, -30, 30, 20, 24), name: "e2e-bracket-\(UUID().uuidString).stl"))
        await model.load(url: stl)
        #expect(model.status == nil, "\(model.status ?? "")")
        let wiring = try #require(model.wiring)
        let build = try #require(wiring.build)
        #expect(build.latest == nil, "nothing is built until Add fins is on")

        // Add fins -> the build -> the Fins row
        model.session.perform("Add Fins") { model.session.finsOn = true }
        try await eventually("the first build") { build.latest != nil && !build.building }
        #expect(build.error == nil, "\(build.error ?? "")")
        let fins = try finsRow(build)
        #expect(fins.contains("fin") || fins.contains("prop") || fins.contains("wall"), "the Fins row shows a count: \(fins)")
        #expect(fins.first?.isNumber == true, "starts with the count: \(fins)")
        #expect(model.controller?.renderer.supports != nil, "the fins are on the viewer")

        // a setting the engine takes -> the debounced rebuild
        let generation = build.generation
        var s = model.session.settings
        s.tineDensity = 60
        model.session.setSettings(s)
        try await eventually("the rebuild") { build.generation == generation + 1 && build.latest != nil && !build.building }
        #expect(build.error == nil, "\(build.error ?? "")")

        // export STL to a temp dir
        let dir = try Scratch.directory()
        let exporter = try #require(wiring.exporter)
        let name = try #require(exporter.fileName(for: .stl))
        let written = try #require(exporter.write(.stl, to: dir.appending(path: name)), "\(exporter.error ?? "")")
        let size = try #require(try FileManager.default.attributesOfItem(atPath: written.path)[.size] as? Int)
        #expect(size > 84 && (size - 84) % 50 == 0, "a binary STL with triangles: \(size) bytes")
        Scratch.remove(dir)
        Scratch.remove(stl)
    }

    /// The text of the stats card's Fins row for the newest build (what `ReadoutSections` shows).
    private func finsRow(_ build: BuildModel) throws -> String {
        let json = try #require(build.readoutSummaryJSON)
        let built = try ReadoutBuilt(summaryJSON: json, finCount: build.readoutFinCount)
        return FinReadout.make(built, ReadoutContext()).fins
    }
}
