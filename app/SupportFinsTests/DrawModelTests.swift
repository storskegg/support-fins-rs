import Foundation
import SupportFinsCore
import Testing
import Viewer
import simd

@testable import Support_Fins

/// Draw headless, through the pieces the window is made of: the pointer is a `HandPointer` (a face and a ray, what the
/// viewer reports) and the clicks are the web's (`web/ui/walls.js` `drawClick`).
@MainActor
@Suite("Draw", .serialized)
struct DrawModelTests {
    private func eventually(_ what: String, _ condition: () -> Bool) async throws {
        for _ in 0..<1500 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        Issue.record("timed out: \(what)")
    }

    /// A window with a T-shaped part open (a stem under a slab: the slab's underside is an overhang), fins on, Draw mode.
    private func drawWindow(mode: FinSettings.Mode = .draw, sway: Bool = false, parts: [[Float]]? = nil) async throws
        -> (window: ViewerModel, draw: DrawModel, kit: DrawKit) {
        UserDefaults.standard.removeObject(forKey: DrawModel.shapeKey)     // a crashed run may have left Curve behind
        let window = ViewerModel()
        try #require(window.controller != nil, "no Metal device")
        let tris = parts ?? (TestParts.box(-5, 5, -5, 5, 0, 20) + TestParts.box(-20, 20, -20, 20, 20, 24))
        let stl = URL(fileURLWithPath: try TestParts.write(tris, name: "draw-\(UUID().uuidString).stl"))
        await window.load(url: stl)
        var s = window.session.settings
        s.mode = mode
        s.sway = sway
        window.session.setSettings(s)
        window.session.perform("Add Fins") { window.session.finsOn = true }
        let draw = try #require(window.features[DrawFeature.self]).model
        draw.sync()
        if draw.drawShown || draw.drawActive || draw.braceActive { try await eventually("the kit") { draw.kitReady } }
        let part = try #require(window.session.parts.first).part
        let kit = try DrawKit(part: part, rot: window.session.pose.columnMajor, thresholdDeg: s.threshold,
                              settingsJson: s.drawSettingsJSON(), tunablesJson: s.tunablesJSON())
        Scratch.remove(stl)
        return (window, draw, kit)
    }

    /// The pointer looking up at the part from below at (`x`, `y`): the nearest face the ray meets, like the id pass.
    private func below(_ kit: DrawKit, faces: Int, x: Double, y: Double) throws -> ViewerController.HandPointer {
        var best: (face: Int, d: Double)?
        for f in 0..<faces {
            if let h = try kit.surfaceHit(face: UInt64(f), origin: [x, y, -50], dir: [0, 0, 1]), best == nil || h.distance < best!.d {
                best = (f, h.distance)
            }
        }
        return ViewerController.HandPointer(face: best.map { (part: 0, face: $0.face) }, origin: SIMD3(x, y, -50), direction: SIMD3(0, 0, 1),
                                            location: CGPoint(x: 400, y: 300), size: CGSize(width: 800, height: 600))
    }

    /// A ray along +y at height `z`, `x` across: for a wall's side, where no face of the part is under it.
    private func side(x: Double, z: Double) -> ViewerController.HandPointer {
        ViewerController.HandPointer(face: nil, origin: SIMD3(x, -60, z), direction: SIMD3(0, 1, 0),
                                     location: CGPoint(x: 400, y: 300), size: CGSize(width: 800, height: 600))
    }

    // MARK: lines

    @Test func twoClicksLayAWallThatIsOneUndoStep() async throws {
        let (window, draw, kit) = try await drawWindow()
        let c = try #require(window.controller)
        #expect(c.handPicking == .walls && draw.drawActive)
        let a = try below(kit, faces: 24, x: -12, y: 12), b = try below(kit, faces: 24, x: 12, y: 12)
        draw.pointerMoved(a)
        draw.pointerClicked(a)
        #expect(draw.started && window.session.drawnWalls.isEmpty, "the first click only starts a wall")
        #expect(c.renderer.drawScene?.markers.isEmpty == false, "its end is a dot")
        draw.pointerMoved(b)
        draw.pointerClicked(b)
        #expect(!draw.started && window.session.drawnWalls.count == 1 && draw.message.isEmpty, "\(draw.message)")
        try #require(window.session.drawnWalls.count == 1)
        let wall = window.session.drawnWalls[0]
        #expect(wall.kind == .wall && wall.m == nil)
        // the wall is stored in the part's own frame; the identity pose seats it on the plate
        #expect(abs(wall.a.x + 12) < 1e-6 && abs(wall.b.x - 12) < 1e-6 && abs(wall.a.y - 12) < 1e-6)
        try await eventually("the sweep") { draw.outcomes[wall.id] != nil }
        let o = try #require(draw.outcomes[wall.id])
        #expect(o.ok && o.length > 20 && o.triEnd > o.triStart)
        // the wall is on the viewer
        try await eventually("the wall on the viewer") { c.renderer.drawScene?.walls != nil }
        // the readout
        let state = try #require(draw.readoutState)
        #expect(state.input.walls.count == 1 && state.input.walls[0].ok && state.added != nil)
        let ctx: ReadoutContext = { var c = ReadoutContext(); c.mode = "draw"; c.draw = state.input; return c }()
        #expect(FinReadout.make(nil, ctx).fins.hasPrefix("1 drawn wall"))
        // one undo takes it back, redo puts it back
        window.session.undo()
        #expect(window.session.drawnWalls.isEmpty)
        window.session.redo()
        #expect(window.session.drawnWalls == [wall])
    }

    @Test func theExportCarriesTheDrawnWallsAndWaitsForTheirSweep() async throws {
        let (window, draw, kit) = try await drawWindow()
        let wiring = try #require(window.wiring)
        let build = try #require(wiring.build)
        try await eventually("Draw's build (the pad only)") { build.latest != nil && !build.building }
        #expect(build.latest?.fins().count() == 0, "Draw builds no walls of its own")
        let exporter = try #require(wiring.exporter)
        draw.pointerClicked(try below(kit, faces: 24, x: -12, y: 12))
        draw.pointerClicked(try below(kit, faces: 24, x: 12, y: 12))
        #expect(exporter.availability.enabled.isEmpty, "not offered while the wall is being stood")
        try await eventually("the sweep") { draw.sweepCurrent }
        #expect(exporter.availability.enabled.contains(.finsOnly))
        let dir = try Scratch.directory()
        let file = try #require(exporter.write(.finsOnly, to: dir.appending(path: "fins.stl")), "\(exporter.error ?? "")")
        let size = try #require(try FileManager.default.attributesOfItem(atPath: file.path)[.size] as? Int)
        let triangles = (size - 84) / 50
        // the wall's triangles, then the pad's (a T on its stem needs none)
        let o = try #require(draw.outcomes.values.first)
        #expect(triangles == (o.triEnd - o.triStart) + Int(build.latest!.pad().count() / 3), "\(triangles) triangles written")
        Scratch.remove(dir)
    }

    @Test func aWallTooShortIsRefusedWithTheWebsWordsAndNothingIsStored() async throws {
        let (window, draw, kit) = try await drawWindow()
        let a = try below(kit, faces: 24, x: -12, y: 12), b = try below(kit, faces: 24, x: -11.5, y: 12)
        draw.pointerClicked(a)
        draw.pointerClicked(b)
        #expect(window.session.drawnWalls.isEmpty && !draw.started)
        #expect(draw.message.hasPrefix("couldn’t place that wall: wall too short"), "\(draw.message)")
    }

    @Test func backOneTakesBackThePointAndEscCancelsTheWallInProgress() async throws {
        let (window, draw, kit) = try await drawWindow()
        defer { withExtendedLifetime(window) {} }        // the window holds the session Draw edits
        let a = try below(kit, faces: 24, x: -12, y: 12)
        #expect(!draw.undoPoint(), "nothing to take back")
        draw.pointerClicked(a)
        #expect(draw.started && draw.canDelete)
        #expect(draw.undoPoint() && !draw.started)
        draw.pointerClicked(a)
        #expect(draw.cancel(escape: false) && !draw.started, "a right click cancels the wall in progress")
        #expect(!draw.cancel(escape: true), "nothing to cancel or deselect")
    }

    // MARK: select, remove, clear

    @Test func aClickOnAWallSelectsItAndDeleteRemovesItAsOneStep() async throws {
        let (window, draw, kit) = try await drawWindow()
        draw.pointerClicked(try below(kit, faces: 24, x: -12, y: 12))
        draw.pointerClicked(try below(kit, faces: 24, x: 12, y: 12))
        let wall = try #require(window.session.drawnWalls.first)
        try await eventually("the sweep") { draw.outcomes[wall.id] != nil && draw.sweepCurrent }
        // a click on the wall (a ray along its side, the part not in front of it) selects it; a second deselects
        draw.pointerClicked(side(x: 10, z: 8))
        #expect(draw.selectedID == wall.id)
        let note = try #require(draw.selectedNote)
        #expect(note.hasPrefix("selected: wall ") && note.contains("mm long") && note.hasSuffix("Press Delete or Remove selected to take it out (Esc to keep it)"), "\(note)")
        draw.pointerClicked(side(x: 10, z: 8))
        #expect(draw.selectedID == nil)
        draw.pointerClicked(side(x: 10, z: 8))
        #expect(draw.canDelete)
        #expect(draw.cancel(escape: true) && draw.selectedID == nil, "Esc deselects")
        draw.pointerClicked(side(x: 10, z: 8))
        #expect(draw.key(.delete))
        #expect(window.session.drawnWalls.isEmpty && draw.selectedID == nil)
        window.session.undo()
        #expect(window.session.drawnWalls.count == 1)
    }

    /// With a section on, a click skips the cut-away half (`keptHit`, `web/ui/section.js`): a wall behind the plane is
    /// not picked, the same wall on the kept side is.
    @Test func aWallTheSectionCutsAwayIsNotPicked() async throws {
        let (window, draw, kit) = try await drawWindow()
        let c = try #require(window.controller)
        draw.pointerClicked(try below(kit, faces: 24, x: -12, y: 12))
        draw.pointerClicked(try below(kit, faces: 24, x: 12, y: 12))
        let wall = try #require(window.session.drawnWalls.first)
        try await eventually("the sweep") { draw.outcomes[wall.id] != nil && draw.sweepCurrent }
        // a cut through y = 0 keeping -y: the wall (y = 12) is on the cut-away side
        func cut(keeping normal: SIMD3<Float>) {
            let turn = simd_quatf(from: SIMD3<Float>(0, 0, 1), to: normal)
            let world = SectionPlane(position: .zero, orientation: turn).matrix
            c.section.restore(c.renderer.modelMatrix.inverse * world)
        }
        cut(keeping: SIMD3(0, -1, 0))
        #expect(c.section.isOn && draw.sectionKeep != nil)
        draw.pointerClicked(side(x: 10, z: 8))
        #expect(draw.selectedID == nil, "cut away: not picked")
        // keeping +y: the wall is in view and a click there picks it
        cut(keeping: SIMD3(0, 1, 0))
        draw.pointerClicked(side(x: 10, z: 8))
        #expect(draw.selectedID == wall.id)
        c.section.setOn(false)
        #expect(draw.sectionKeep == nil)
    }

    @Test func clearAllIsOneUndoStepAndDoesNothingWithoutWalls() async throws {
        let (window, draw, kit) = try await drawWindow()
        draw.clearAll()                                     // nothing to clear: no undo step
        let steps = window.session.history.undoStack.count
        for x in [-12.0, 0.0] {
            draw.pointerClicked(try below(kit, faces: 24, x: x, y: 8))
            draw.pointerClicked(try below(kit, faces: 24, x: x + 8, y: 8))
        }
        #expect(window.session.drawnWalls.count == 2 && window.session.history.undoStack.count == steps + 2)
        draw.clearAll()
        #expect(window.session.drawnWalls.isEmpty)
        window.session.undo()
        #expect(window.session.drawnWalls.count == 2, "one step brings both back")
    }

    // MARK: curves

    @Test func aCurveIsTwoEndsAndAPointItRunsThroughAndRestartsWhenTheShapeChanges() async throws {
        let (window, draw, kit) = try await drawWindow()
        draw.setShape(.curve)
        defer { draw.setShape(.line) }
        draw.pointerClicked(try below(kit, faces: 24, x: -12, y: -4))
        draw.pointerClicked(try below(kit, faces: 24, x: 12, y: -4))
        #expect(draw.started && window.session.drawnWalls.isEmpty, "two ends are not a curve yet")
        draw.pointerClicked(try below(kit, faces: 24, x: 0, y: 12))
        #expect(window.session.drawnWalls.count == 1)
        let wall = window.session.drawnWalls[0]
        #expect(wall.m != nil && !draw.started)
        try await eventually("the sweep") { draw.outcomes[wall.id] != nil }
        #expect(draw.outcomes[wall.id]?.ok == true)
        // changing the shape starts a half-drawn wall over
        draw.pointerClicked(try below(kit, faces: 24, x: -12, y: -10))
        #expect(draw.started)
        draw.setShape(.line)
        #expect(!draw.started)
        #expect(draw.hint.hasPrefix("Click two points across an overhang — straight onto the red faces"))
        draw.setShape(.curve)
        #expect(draw.hint.hasPrefix("Click the two ends of a curved wall on the red faces"))
    }

    @Test func aCurveTooTightKeepsItsEndsAndSaysWhy() async throws {
        let (window, draw, kit) = try await drawWindow()
        draw.setShape(.curve)
        defer { draw.setShape(.line) }
        draw.pointerClicked(try below(kit, faces: 24, x: -3, y: 8))
        draw.pointerClicked(try below(kit, faces: 24, x: 3, y: 8))
        draw.pointerClicked(try below(kit, faces: 24, x: 0, y: 11))         // 6 mm chord, 3 mm bulge: radius 3
        #expect(window.session.drawnWalls.isEmpty && draw.started, "the ends are kept: the next click tries a wider bend")
        #expect(draw.message.hasPrefix("can’t place this curve: curve too tight"), "\(draw.message)")
        draw.pointerClicked(try below(kit, faces: 24, x: 0, y: 17.5))       // a wider bend: radius 5.2
        #expect(window.session.drawnWalls.count == 1 && draw.message.isEmpty)
    }

    // MARK: modes

    @Test func theAugmentToggleIsAutoAndFullsAndEndsWithTheMode() async throws {
        let (window, draw, _) = try await drawWindow(mode: .auto)
        let c = try #require(window.controller)
        #expect(draw.augmentOffered && !draw.drawActive && c.handPicking == .off)
        draw.toggleAugment()
        #expect(draw.drawActive && c.handPicking == .walls && draw.augmentTitle == "Done adding walls")
        // a mode switch starts with hand-placement off
        var s = window.session.settings
        s.mode = .full
        window.session.setSettings(s)
        draw.sync()
        #expect(!window.session.addByHand && c.handPicking == .off && draw.augmentTitle == "+ Add walls by hand")
        // fins off ends it too
        draw.toggleAugment()
        window.session.perform("Remove Fins") { window.session.finsOn = false }
        draw.sync()
        #expect(!window.session.addByHand && !draw.augmentOffered && c.handPicking == .off)
    }

    @Test func handPlacedWallsStayShownInAutoAfterTheToggleIsOff() async throws {
        let (window, draw, kit) = try await drawWindow(mode: .auto)
        draw.toggleAugment()
        try await eventually("the kit") { draw.kitReady }
        draw.pointerClicked(try below(kit, faces: 24, x: -12, y: 12))
        draw.pointerClicked(try below(kit, faces: 24, x: 12, y: 12))
        #expect(window.session.drawnWalls.count == 1)
        draw.toggleAugment()
        #expect(!draw.drawActive && draw.drawShown && draw.controlsShown && !draw.hintShown, "Clear stays, the click hint goes")
        // a new part forgets them
        draw.partChanged()
        window.session.replaceParts(window.session.parts)
        #expect(window.session.drawnWalls.isEmpty)
    }

    // MARK: sway braces

    @Test func aClickOnAnUprightSideStandsABraceWhenSwayBracesAreOn() async throws {
        let post = TestParts.box(-20, 20, -15, 15, 0, 100)
        let (window, draw, kit) = try await drawWindow(mode: .auto, sway: true, parts: post)
        let c = try #require(window.controller)
        #expect(draw.braceActive && c.handPicking == .braces)
        // the pointer on the x = -20 side, a third of the way up
        var face: Int?
        for f in 0..<12 where (try kit.surfaceHit(face: UInt64(f), origin: [-50, 0, 30], dir: [1, 0, 0])) != nil { face = f; break }
        let hit = ViewerController.HandPointer(face: face.map { (part: 0, face: $0) }, origin: SIMD3(-50, 0, 30), direction: SIMD3(1, 0, 0),
                                               location: CGPoint(x: 300, y: 200), size: CGSize(width: 800, height: 600))
        #expect(kit.faceTakesBrace(face: UInt64(try #require(face))))
        draw.pointerMoved(hit)
        try await eventually("the preview") { !draw.motion.ghosts.isEmpty }
        #expect(draw.motion.tip.text.hasPrefix("Brace · "), "\(draw.motion.tip.text)")
        draw.pointerClicked(hit)
        #expect(window.session.drawnWalls.count == 1 && window.session.drawnWalls[0].kind == .sway)
        let brace = window.session.drawnWalls[0]
        try await eventually("the sweep") { draw.outcomes[brace.id] != nil }
        #expect(draw.outcomes[brace.id]?.ok == true)
        let state = try #require(draw.readoutState)
        let ctx: ReadoutContext = { var c = ReadoutContext(); c.mode = "draw"; c.draw = state.input; return c }()
        #expect(FinReadout.make(nil, ctx).fins.hasPrefix("1 sway brace"))
        window.session.undo()
        #expect(window.session.drawnWalls.isEmpty)
    }
}
