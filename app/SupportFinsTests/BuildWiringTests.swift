import Foundation
import Metal
import SupportFinsCore
import Testing
import Viewer

@testable import Support_Fins

private func box(_ x0: Float, _ x1: Float, _ y0: Float, _ y1: Float, _ z0: Float, _ z1: Float) -> [[Float]] {
    let v: [[Float]] = [
        [x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0],
        [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1],
    ]
    let faces = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]]
    return faces.map { f in f.flatMap { v[$0] } }
}

private func tURL() throws -> URL {
    let tris = box(-5, 5, -5, 5, 0, 20) + box(-30, 30, -30, 30, 20, 24)
    var data = Data(count: 80)
    var count = UInt32(tris.count)
    withUnsafeBytes(of: &count) { data.append(contentsOf: $0) }
    for t in tris {
        for f in [Float](repeating: 0, count: 3) + t { var x = f; withUnsafeBytes(of: &x) { data.append(contentsOf: $0) } }
        data.append(contentsOf: [0, 0])
    }
    let url = URL(fileURLWithPath: NSTemporaryDirectory() + "wiring-tests-bracket.stl")
    try data.write(to: url)
    return url
}

/// A loaded window: the session, the viewer, the wiring and the clock that drives the debounce.
@MainActor
private struct Window {
    let session: PartSession
    let controller: ViewerController
    let wiring: BuildWiring
    let clock: ManualClock

    static func open() async throws -> Window {
        let controller = try #require(ViewerController(), "no Metal device")
        let session = PartSession()
        let clock = ManualClock()
        let wiring = BuildWiring(session: session, controller: controller, sleep: { try await clock.sleep($0) })
        let loaded = try await CoreLoader().load(url: try tURL(), device: controller.renderer.device)
        session.replaceParts(loaded.compactMap { $0.mesh as? CorePart })
        wiring.attach()
        controller.show(loaded.map(\.mesh), name: "bracket.stl")
        return Window(session: session, controller: controller, wiring: wiring, clock: clock)
    }

    var build: BuildModel { wiring.build! }

    /// Waits (a few seconds at most) for `condition`, letting the main actor run.
    func eventually(_ what: String = "", _ condition: () -> Bool) async throws {
        for _ in 0..<500 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        Issue.record("timed out: \(what)")
    }

    /// Fins on, and the first build landed on the viewer.
    func finsOn() async throws {
        session.perform("Add Fins") { session.finsOn = true }
        try await eventually("the first build") { build.latest != nil && !build.building && controller.renderer.supports != nil }
    }
}

@MainActor
@Suite("Window wiring")
struct BuildWiringTests {
    @Test func nothingIsBuiltUntilAddFinsIsOn() async throws {
        let w = try await Window.open()
        await w.clock.advance(.seconds(2))
        #expect(w.build.generation == 0 && w.build.latest == nil && w.controller.renderer.supports == nil)
        #expect(w.wiring.exporter?.availability.enabled.isEmpty == true)
    }

    @Test func turningFinsOnBuildsAtOnceAndPutsThemOnTheViewer() async throws {
        let w = try await Window.open()
        try await w.finsOn()
        #expect(w.build.generation == 1 && w.build.error == nil)
        let layer = try #require(w.controller.renderer.supports)
        #expect(layer.finVertices == Int(w.build.latest!.fins().count()) && layer.finVertices > 0)
        #expect(w.controller.renderer.hiddenFinRanges.isEmpty)
        // the build used the session's pose and the settings' options
        #expect(w.wiring.exporter?.availability.enabled.isEmpty == false)
    }

    @Test func aPoseChangeRebuildsOnceTheDebounceHasPassed() async throws {
        let w = try await Window.open()
        try await w.finsOn()
        let before = w.build.generation
        w.session.rotate90(about: .x)
        try await w.eventually("the change is seen") { w.session.pose != .identity }
        await w.clock.advance(.milliseconds(179))
        #expect(w.build.generation == before, "179 ms after the change: not yet")
        await w.clock.advance(.milliseconds(1))
        try await w.eventually("the rebuild") { w.build.generation == before + 1 }
        try await w.eventually("it lands") { !w.build.building }
        #expect(w.build.summary != nil && w.controller.renderer.dimSupports == false)
    }

    @Test func aSettingTheEngineIgnoresDoesNotRebuild() async throws {
        let w = try await Window.open()
        try await w.finsOn()
        let before = w.build.generation
        var s = w.session.settings
        s.highlightSmall.toggle()                 // paints the part, never the fins
        w.session.setSettings(s)
        await w.clock.advance(.seconds(2))
        try await Task.sleep(for: .milliseconds(100))
        #expect(w.build.generation == before)
        // one the engine takes does
        s.tineDensity = 50
        w.session.setSettings(s)
        try await Task.sleep(for: .milliseconds(100))
        await w.clock.advance(.milliseconds(180))
        try await w.eventually("the options rebuild") { w.build.generation == before + 1 }
    }

    @Test func removingAFinIsUndoableAndNeverRebuilds() async throws {
        let w = try await Window.open()
        try await w.finsOn()
        let removal = try #require(w.wiring.removal)
        let generation = w.build.generation
        removal.arm()
        w.wiring.show()
        #expect(w.controller.supportPicking)

        let fin = try #require(w.build.records?.fins.first)
        w.controller.onSupportClick?(fin.triRanges[0].from / 3)       // the viewer's click
        #expect(w.session.removedFins.count == 1 && w.session.canUndo)
        #expect(w.controller.renderer.hiddenFinRanges == w.build.ranges(ofFin: fin.id))
        #expect(w.build.generation == generation, "a removal only masks")

        w.session.undo()
        try await w.eventually("undo shows the fin again") { w.controller.renderer.hiddenFinRanges.isEmpty }
        #expect(w.session.removedFins.isEmpty)
        w.session.redo()
        try await w.eventually("redo hides it") { w.controller.renderer.hiddenFinRanges == w.build.ranges(ofFin: fin.id) }

        // Restore all is a step too
        removal.restoreAll()
        try await w.eventually("restore shows it") { w.controller.renderer.hiddenFinRanges.isEmpty }
        w.session.undo()
        try await w.eventually("undo of restore hides it again") { !w.controller.renderer.hiddenFinRanges.isEmpty }
    }

    @Test func fins_offClearsTheViewerAndEndsRemoveMode() async throws {
        let w = try await Window.open()
        try await w.finsOn()
        let removal = try #require(w.wiring.removal)
        removal.arm()
        w.wiring.show()
        w.session.perform("Remove Fins") { w.session.finsOn = false }
        try await w.eventually("fins go") { w.controller.renderer.supports == nil }
        #expect(w.build.latest == nil && !removal.armed && !w.controller.supportPicking)
        // undo brings the fins back, and builds them
        w.session.undo()
        try await w.eventually("undo of the toggle") { w.session.finsOn && w.controller.renderer.supports != nil }
    }

    @Test func escEndsRemoveModeAndOtherwiseIsLeftAlone() async throws {
        let w = try await Window.open()
        try await w.finsOn()
        #expect(!w.wiring.escape(), "not armed: Esc is someone else's")
        w.wiring.removal?.arm()
        w.wiring.show()
        #expect(w.controller.supportPicking)
        #expect(w.wiring.escape())
        #expect(w.wiring.removal?.armed == false && !w.controller.supportPicking)
    }

    @Test func drawModeEndsRemoveMode() async throws {
        let w = try await Window.open()
        try await w.finsOn()
        let removal = try #require(w.wiring.removal)
        removal.arm()
        var s = w.session.settings
        s.mode = .draw
        w.session.setSettings(s)
        try await w.eventually("remove mode ends") { !removal.armed }
    }

    @Test func exportNamesTheFileForTheSettingsTheFinsWereBuiltWith() async throws {
        let w = try await Window.open()
        try await w.finsOn()
        let ex = try #require(w.wiring.exporter)
        #expect(ex.fileName(for: .stl) == "wiring-tests-bracket-fins-0.4nozzle-0.2layer.stl")
        var s = w.session.settings
        s.nozzle = 0.6
        w.session.setSettings(s)
        #expect(ex.fileName(for: .stl) == "wiring-tests-bracket-fins-0.6nozzle-0.2layer.stl")
        #expect(ex.multiObjectNote == nil)
    }

    @Test func aNewFileStartsOver() async throws {
        let w = try await Window.open()
        try await w.finsOn()
        let first = try #require(w.wiring.build)
        let loaded = try await CoreLoader().load(url: try tURL(), device: w.controller.renderer.device)
        w.session.replaceParts(loaded.compactMap { $0.mesh as? CorePart })
        w.wiring.attach()
        #expect(w.wiring.build !== first && w.session.removedFins.isEmpty)
        // the toggle is still on: the new file's fins are built
        try await w.eventually("the new file's fins") { w.wiring.build?.latest != nil && w.controller.renderer.supports != nil }
    }
}
