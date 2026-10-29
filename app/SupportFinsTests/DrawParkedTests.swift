import Foundation
import SupportFinsCore
import Testing
import Viewer
import simd

@testable import Support_Fins

/// A model's hand-placed walls go with it when another is added (`ui/models.js` `park` takes its state with
/// `takeState`), stand beside it on the plate while it is parked (`ui/parkedbuild.js`), and come back when it does.
@MainActor
@Suite("Draw and several models", .serialized)
struct DrawParkedTests {
    private func eventually(_ what: String, _ condition: () -> Bool) async throws {
        for _ in 0..<1500 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        Issue.record("timed out: \(what)")
    }

    private func stl(_ name: String) throws -> URL {
        URL(fileURLWithPath: try TestParts.write(
            TestParts.box(-5, 5, -5, 5, 0, 20) + TestParts.box(-30, 30, -30, 30, 20, 24), name: "\(name)-\(UUID().uuidString).stl"))
    }

    @Test func parkingAModelKeepsItsWallsAndBringingItBackRestoresThemAndTheNewOneStartsClean() async throws {
        UserDefaults.standard.removeObject(forKey: DrawModel.shapeKey)
        let window = ViewerModel()
        try #require(window.controller != nil, "no Metal device")
        await window.load(url: try stl("park-first"))
        let board = try #require(window.features[PlatesFeature.self]?.board)
        let draw = try #require(window.features[DrawFeature.self])
        var s = window.session.settings
        s.mode = .draw
        window.session.setSettings(s)
        window.session.perform("Add Fins") { window.session.finsOn = true }
        draw.model.sync()
        try await eventually("the kit") { draw.model.kitReady }
        // a wall across the slab's underside, clear of the stem
        let wall = DrawnWall.wall(from: SIMD3(-12, 12, 20), to: SIMD3(12, 12, 20))
        window.session.perform("Draw Wall") { window.session.drawnWalls.append(wall) }
        draw.model.sync()
        try await eventually("the sweep") { draw.model.sweepCurrent }
        #expect(draw.model.outcomes[wall.id]?.ok == true)

        // add a second model: the first is parked with its wall, the second shows none
        await window.load(url: try stl("park-second"), adding: true)
        let first = board.plate.models[0]
        #expect(board.plate.live == 1 && first.stash != nil)
        #expect(window.session.drawnWalls.isEmpty, "the new model has no walls")
        let stash = try #require(first.stash)
        let kept = try #require(stash.featureStates[ObjectIdentifier(DrawFeature.self)] as? ParkedWalls)
        #expect(kept.walls == [wall])

        // while parked its wall is stood for its pose and shown with it
        var shown: (sweep: DrawnSweep?, mesh: OverlayMesh?)?
        try await eventually("the parked wall") {
            shown = draw.parkedDrawn(stash) { board.sync() }
            return shown?.mesh != nil
        }
        #expect((shown?.sweep?.triangles().count() ?? 0) > 0)
        #expect(window.controller?.renderer.plateScene.parked.contains { $0.drawn != nil } == true)

        // back to it: the wall and the augment state return, and the second model's own stay on it
        board.swap(to: 0)
        #expect(window.session.drawnWalls == [wall])
        #expect(board.plate.models[1].stash?.featureStates[ObjectIdentifier(DrawFeature.self)] == nil, "the second had none")
        try await eventually("the sweep again") { draw.model.sweepCurrent && draw.model.outcomes[wall.id] != nil }
        #expect(draw.model.outcomes[wall.id]?.ok == true)
    }
}
