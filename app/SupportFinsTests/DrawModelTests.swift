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
    /// A click, and the engine answer it waits for (a wall or brace check runs off the main thread).
    private func click(_ draw: DrawModel, _ p: ViewerController.HandPointer) async throws {
        draw.pointerClicked(p)
        try await eventually("the click's check", tries: 1500) { !draw.clickPending }
    }

    /// A window with a T-shaped part open (a stem under a slab: the slab's underside is an overhang), fins on, Draw mode.
    private func drawWindow(mode: FinSettings.Mode = .draw, sway: Bool = false, parts: [[Float]]? = nil, file: URL? = nil) async throws
        -> (window: ViewerModel, draw: DrawModel, kit: DrawKit) {
        UserDefaults.standard.removeObject(forKey: DrawModel.shapeKey)     // a crashed run may have left Curve behind
        let window = ViewerModel()
        try #require(window.controller != nil, "no Metal device")
        let tris = parts ?? (TestParts.box(-5, 5, -5, 5, 0, 20) + TestParts.box(-20, 20, -20, 20, 20, 24))
        let stl = try file ?? URL(fileURLWithPath: TestParts.write(tris, name: "draw-\(UUID().uuidString).stl"))
        await window.load(url: stl)
        var s = window.session.settings
        s.mode = mode
        s.sway = sway
        window.session.setSettings(s)
        window.session.perform("Add Fins") { window.session.finsOn = true }
        let draw = try #require(window.features[DrawFeature.self]).model
        draw.sync()
        if draw.drawShown || draw.drawActive || draw.braceActive { try await eventually("the kit", tries: 1500) { draw.kitReady } }
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
        try await click(draw, a)
        #expect(draw.started && window.session.drawnWalls.isEmpty, "the first click only starts a wall")
        #expect(c.renderer.drawScene?.markers.isEmpty == false, "its end is a dot")
        draw.pointerMoved(b)
        try await click(draw, b)
        #expect(!draw.started && window.session.drawnWalls.count == 1 && draw.message.isEmpty, "\(draw.message)")
        try #require(window.session.drawnWalls.count == 1)
        let wall = window.session.drawnWalls[0]
        #expect(wall.kind == .wall && wall.m == nil)
        // the wall is stored in the part's own frame; the identity pose seats it on the plate
        #expect(abs(wall.a.x + 12) < 1e-6 && abs(wall.b.x - 12) < 1e-6 && abs(wall.a.y - 12) < 1e-6)
        try await eventually("the sweep", tries: 1500) { draw.outcomes[wall.id] != nil }
        let o = try #require(draw.outcomes[wall.id])
        #expect(o.ok && o.length > 20 && o.triEnd > o.triStart)
        // the wall is on the viewer
        try await eventually("the wall on the viewer", tries: 1500) { c.renderer.drawScene?.walls != nil }
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
        try await eventually("Draw's build (the pad only)", tries: 1500) { build.latest != nil && !build.building }
        #expect(build.latest?.fins().count() == 0, "Draw builds no walls of its own")
        let exporter = try #require(wiring.exporter)
        try await click(draw, try below(kit, faces: 24, x: -12, y: 12))
        try await click(draw, try below(kit, faces: 24, x: 12, y: 12))
        if !draw.sweepCurrent { #expect(exporter.availability.enabled.isEmpty, "not offered while the wall is being stood") }
        try await eventually("the sweep", tries: 1500) { draw.sweepCurrent }
        #expect(exporter.availability.enabled.contains(.finsOnly))
        let dir = try Scratch.directory()
        let file = try #require(await exporter.write(.finsOnly, to: dir.appending(path: "fins.stl")), "\(exporter.error ?? "")")
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
        try await click(draw, a)
        try await click(draw, b)
        #expect(window.session.drawnWalls.isEmpty && !draw.started)
        #expect(draw.message.hasPrefix("couldn’t place that wall: wall too short"), "\(draw.message)")
    }

    @Test func backOneTakesBackThePointAndEscCancelsTheWallInProgress() async throws {
        let (window, draw, kit) = try await drawWindow()
        defer { withExtendedLifetime(window) {} }        // the window holds the session Draw edits
        let a = try below(kit, faces: 24, x: -12, y: 12)
        #expect(!draw.undoPoint(), "nothing to take back")
        try await click(draw, a)
        #expect(draw.started && draw.canDelete)
        #expect(draw.undoPoint() && !draw.started)
        try await click(draw, a)
        #expect(draw.cancel(escape: false) && !draw.started, "a right click cancels the wall in progress")
        #expect(!draw.cancel(escape: true), "nothing to cancel or deselect")
    }

    // MARK: select, remove, clear

    @Test func aClickOnAWallSelectsItAndDeleteRemovesItAsOneStep() async throws {
        let (window, draw, kit) = try await drawWindow()
        try await click(draw, try below(kit, faces: 24, x: -12, y: 12))
        try await click(draw, try below(kit, faces: 24, x: 12, y: 12))
        let wall = try #require(window.session.drawnWalls.first)
        try await eventually("the sweep", tries: 1500) { draw.outcomes[wall.id] != nil && draw.sweepCurrent }
        // a click on the wall (a ray along its side, the part not in front of it) selects it; a second deselects
        try await click(draw, side(x: 10, z: 8))
        #expect(draw.selectedID == wall.id)
        let note = try #require(draw.selectedNote)
        #expect(note.hasPrefix("selected: wall ") && note.contains("mm long") && note.hasSuffix("Press Delete or Remove selected to take it out (Esc to keep it)"), "\(note)")
        try await click(draw, side(x: 10, z: 8))
        #expect(draw.selectedID == nil)
        try await click(draw, side(x: 10, z: 8))
        #expect(draw.canDelete)
        #expect(draw.cancel(escape: true) && draw.selectedID == nil, "Esc deselects")
        try await click(draw, side(x: 10, z: 8))
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
        try await click(draw, try below(kit, faces: 24, x: -12, y: 12))
        try await click(draw, try below(kit, faces: 24, x: 12, y: 12))
        let wall = try #require(window.session.drawnWalls.first)
        try await eventually("the sweep", tries: 1500) { draw.outcomes[wall.id] != nil && draw.sweepCurrent }
        // a cut through y = 0 keeping -y: the wall (y = 12) is on the cut-away side
        func cut(keeping normal: SIMD3<Float>) {
            let turn = simd_quatf(from: SIMD3<Float>(0, 0, 1), to: normal)
            let world = SectionPlane(position: .zero, orientation: turn).matrix
            c.section.restore(c.renderer.modelMatrix.inverse * world)
        }
        cut(keeping: SIMD3(0, -1, 0))
        #expect(c.section.isOn && draw.sectionKeep != nil)
        try await click(draw, side(x: 10, z: 8))
        #expect(draw.selectedID == nil, "cut away: not picked")
        // keeping +y: the wall is in view and a click there picks it
        cut(keeping: SIMD3(0, 1, 0))
        try await click(draw, side(x: 10, z: 8))
        #expect(draw.selectedID == wall.id)
        c.section.setOn(false)
        #expect(draw.sectionKeep == nil)
    }

    @Test func clearAllIsOneUndoStepAndDoesNothingWithoutWalls() async throws {
        let (window, draw, kit) = try await drawWindow()
        draw.clearAll()                                     // nothing to clear: no undo step
        let steps = window.session.history.undoStack.count
        for x in [-12.0, 0.0] {
            try await click(draw, try below(kit, faces: 24, x: x, y: 8))
            try await click(draw, try below(kit, faces: 24, x: x + 8, y: 8))
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
        try await click(draw, try below(kit, faces: 24, x: -12, y: -4))
        try await click(draw, try below(kit, faces: 24, x: 12, y: -4))
        #expect(draw.started && window.session.drawnWalls.isEmpty, "two ends are not a curve yet")
        try await click(draw, try below(kit, faces: 24, x: 0, y: 12))
        #expect(window.session.drawnWalls.count == 1)
        let wall = window.session.drawnWalls[0]
        #expect(wall.m != nil && !draw.started)
        try await eventually("the sweep", tries: 1500) { draw.outcomes[wall.id] != nil }
        #expect(draw.outcomes[wall.id]?.ok == true)
        // changing the shape starts a half-drawn wall over
        try await click(draw, try below(kit, faces: 24, x: -12, y: -10))
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
        try await click(draw, try below(kit, faces: 24, x: -3, y: 8))
        try await click(draw, try below(kit, faces: 24, x: 3, y: 8))
        try await click(draw, try below(kit, faces: 24, x: 0, y: 11))         // 6 mm chord, 3 mm bulge: radius 3
        #expect(window.session.drawnWalls.isEmpty && draw.started, "the ends are kept: the next click tries a wider bend")
        #expect(draw.message.hasPrefix("can’t place this curve: curve too tight"), "\(draw.message)")
        try await click(draw, try below(kit, faces: 24, x: 0, y: 17.5))       // a wider bend: radius 5.2
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
        try await eventually("the kit", tries: 1500) { draw.kitReady }
        try await click(draw, try below(kit, faces: 24, x: -12, y: 12))
        try await click(draw, try below(kit, faces: 24, x: 12, y: 12))
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
        #expect(try kit.faceTakesBrace(face: UInt64(try #require(face))))
        draw.pointerMoved(hit)
        try await eventually("the preview", tries: 1500) { !draw.motion.ghosts.isEmpty }
        #expect(draw.motion.tip.text.hasPrefix("Brace · "), "\(draw.motion.tip.text)")
        try await click(draw, hit)
        #expect(window.session.drawnWalls.count == 1 && window.session.drawnWalls[0].kind == .sway)
        let brace = window.session.drawnWalls[0]
        try await eventually("the sweep", tries: 1500) { draw.outcomes[brace.id] != nil }
        #expect(draw.outcomes[brace.id]?.ok == true)
        let state = try #require(draw.readoutState)
        let ctx: ReadoutContext = { var c = ReadoutContext(); c.mode = "draw"; c.draw = state.input; return c }()
        #expect(FinReadout.make(nil, ctx).fins.hasPrefix("1 sway brace"))
        window.session.undo()
        #expect(window.session.drawnWalls.isEmpty)
    }

    // MARK: click latency (finding C-08)

    @Test func aWallClickIsAnsweredOffTheMainThreadAndAStaleAnswerIsDropped() async throws {
        let (window, draw, kit) = try await drawWindow()
        defer { withExtendedLifetime(window) {} }
        let a = try below(kit, faces: 24, x: -12, y: 12), b = try below(kit, faces: 24, x: 12, y: 12)
        let c = try below(kit, faces: 24, x: -12, y: 4), d = try below(kit, faces: 24, x: 12, y: 4)
        draw.pointerClicked(a)
        draw.pointerClicked(b)
        #expect(draw.clickPending && draw.started && window.session.drawnWalls.isEmpty, "the wall waits for the engine")
        draw.pointerClicked(c)                     // made while the wall is checked: it runs after the answer
        try await eventually("the check", tries: 1500) { !draw.clickPending }
        #expect(window.session.drawnWalls.count == 1, "the wall lands")
        #expect(draw.started, "the click queued behind it starts the next wall")
        // Esc while the check runs: its answer must not place the wall
        draw.pointerClicked(d)
        #expect(draw.clickPending)
        #expect(draw.cancel(escape: false) && !draw.clickPending)
        try await Task.sleep(for: .milliseconds(300))
        #expect(window.session.drawnWalls.count == 1 && !draw.started, "a cancelled click lands nothing")
        // the walls changing under it (Undo) drops it too, and the clicks queued behind it
        draw.pointerClicked(c)
        draw.pointerClicked(d)
        draw.pointerClicked(a)
        #expect(draw.clickPending)
        window.session.undo()
        try await eventually("the dropped answer", tries: 1500) { !draw.clickPending }
        try await Task.sleep(for: .milliseconds(100))
        #expect(window.session.drawnWalls.isEmpty, "an answer for the old walls is dropped")
        // Back while it runs takes the point back, and the answer goes with it
        #expect(draw.started, "the wall's start stays")
        draw.pointerClicked(d)
        #expect(draw.clickPending && draw.undoPoint() && !draw.clickPending && !draw.started)
        try await Task.sleep(for: .milliseconds(300))
        #expect(window.session.drawnWalls.isEmpty)
    }

    @Test func aBraceClickIsAnsweredOffTheMainThreadAndDroppedWhenBracesGoOff() async throws {
        let post = TestParts.box(-20, 20, -15, 15, 0, 100)
        let (window, draw, kit) = try await drawWindow(mode: .auto, sway: true, parts: post)
        var face: Int?
        for f in 0..<12 where (try kit.surfaceHit(face: UInt64(f), origin: [-50, 0, 30], dir: [1, 0, 0])) != nil { face = f; break }
        let hit = ViewerController.HandPointer(face: face.map { (part: 0, face: $0) }, origin: SIMD3(-50, 0, 30), direction: SIMD3(1, 0, 0),
                                               location: CGPoint(x: 300, y: 200), size: CGSize(width: 800, height: 600))
        draw.pointerClicked(hit)
        #expect(draw.clickPending && window.session.drawnWalls.isEmpty, "the brace waits for the engine")
        try await eventually("the brace", tries: 1500) { !draw.clickPending }
        #expect(window.session.drawnWalls.count == 1 && window.session.drawnWalls[0].kind == .sway)
        // Sway braces off while the next is stood: it is not added
        draw.pointerClicked(hit)
        #expect(draw.clickPending)
        var s = window.session.settings
        s.sway = false
        window.session.setSettings(s)
        draw.sync()
        try await eventually("the dropped answer", tries: 1500) { !draw.clickPending }
        try await Task.sleep(for: .milliseconds(100))
        #expect(window.session.drawnWalls.count == 1, "an answer for braces turned off is dropped")
    }

    /// A part of boxes each side of which is split into `n` x `n` quads (two triangles each), as a binary STL: faces come
    /// box by box, side by side (-z, +z, -x, +x, -y, +y), cell by cell, so a point's face is found without a search.
    private struct GridPart {
        struct Box { var lo: SIMD3<Double>; var hi: SIMD3<Double>; var n: Int }
        let boxes: [Box]
        var faces: Int { boxes.reduce(0) { $0 + 12 * $1.n * $1.n } }

        /// Side `s`'s corner and edges, with `e1 x e2` pointing out.
        func side(_ b: Box, _ s: Int) -> (o: SIMD3<Double>, e1: SIMD3<Double>, e2: SIMD3<Double>) {
            let d = b.hi - b.lo, X = SIMD3(d.x, 0, 0), Y = SIMD3(0, d.y, 0), Z = SIMD3(0, 0, d.z)
            switch s {
            case 0: return (b.lo, Y, X)
            case 1: return (SIMD3(b.lo.x, b.lo.y, b.hi.z), X, Y)
            case 2: return (b.lo, Z, Y)
            case 3: return (SIMD3(b.hi.x, b.lo.y, b.lo.z), Y, Z)
            case 4: return (b.lo, X, Z)
            default: return (SIMD3(b.lo.x, b.hi.y, b.lo.z), Z, X)
            }
        }

        func write() throws -> URL {
            var data = Data(count: 80)
            var count = UInt32(faces)
            withUnsafeBytes(of: &count) { data.append(contentsOf: $0) }
            data.reserveCapacity(84 + faces * 50)
            var rec = [Float](repeating: 0, count: 12)
            for b in boxes {
                for s in 0..<6 {
                    let (o, e1, e2) = side(b, s), n = Double(b.n)
                    for i in 0..<b.n {
                        for j in 0..<b.n {
                            func p(_ di: Int, _ dj: Int) -> SIMD3<Double> { o + e1 * (Double(i + di) / n) + e2 * (Double(j + dj) / n) }
                            for tri in [[p(0, 0), p(1, 0), p(1, 1)], [p(0, 0), p(1, 1), p(0, 1)]] {
                                for (k, v) in tri.enumerated() { rec[3 + 3 * k] = Float(v.x); rec[4 + 3 * k] = Float(v.y); rec[5 + 3 * k] = Float(v.z) }
                                rec.withUnsafeBytes { data.append(contentsOf: $0) }
                                data.append(contentsOf: [0, 0])
                            }
                        }
                    }
                }
            }
            let url = try Scratch.directory().appendingPathComponent("grid-\(UUID().uuidString).stl")
            try data.write(to: url)
            return url
        }

        /// The pointer on side `s` of box `bi` at `at`, the ray from `origin`: the face the id pass would report.
        func pointer(_ kit: DrawKit, box bi: Int, side s: Int, at: SIMD3<Double>, origin: SIMD3<Double>) throws -> ViewerController.HandPointer {
            let b = boxes[bi], (o, e1, e2) = side(b, s)
            let base = boxes[..<bi].reduce(0) { $0 + 12 * $1.n * $1.n } + s * 2 * b.n * b.n
            let ci = Int(simd_dot(at - o, e1) / simd_length_squared(e1) * Double(b.n))
            let cj = Int(simd_dot(at - o, e2) / simd_length_squared(e2) * Double(b.n))
            let dir = simd_normalize(at - origin)
            var best: (face: Int, d: Double)?
            for i in max(0, ci - 1)...min(b.n - 1, ci + 1) {
                for j in max(0, cj - 1)...min(b.n - 1, cj + 1) {
                    for f in [base + 2 * (i * b.n + j), base + 2 * (i * b.n + j) + 1] {
                        if let h = try kit.surfaceHit(face: UInt64(f), origin: [origin.x, origin.y, origin.z], dir: [dir.x, dir.y, dir.z]),
                           best == nil || h.distance < best!.d { best = (f, h.distance) }
                    }
                }
            }
            let f = try #require(best).face
            return ViewerController.HandPointer(face: (part: 0, face: f), origin: origin, direction: dir, location: CGPoint(x: 400, y: 300),
                                                size: CGSize(width: 800, height: 600))
        }
    }

    /// The longest the main actor went without running a job, between `start()` and `stop()`: what a click costs the UI,
    /// its landing included.
    @MainActor private final class Heartbeat {
        private var last = ContinuousClock.now
        private(set) var worst = Duration.zero
        private var task: Task<Void, Never>?
        func start() {
            last = .now; worst = .zero
            task = Task { @MainActor [self] in
                while !Task.isCancelled {
                    let now = ContinuousClock.now
                    worst = max(worst, now - last)
                    last = now
                    await Task.yield()
                }
            }
        }
        func stop() -> Double {
            task?.cancel(); task = nil
            worst = max(worst, .now - last)
            return Self.ms(worst)
        }
        static func ms(_ d: Duration) -> Double { Double(d.components.seconds) * 1000 + Double(d.components.attoseconds) / 1e15 }
    }

    /// Accept of finding C-08: on the 1.2M-face part a click costs the main thread under a frame (16 ms), the first brace
    /// (which builds the triangle index) and a wall's check included, and the answer's landing too.
    @Test func clicksOnA1200000FacePartTakeUnderAFrameOnTheMainThread() async throws {
        // the T of the other tests (a stem under a slab) and a post each side (the part stays centred), 1.2M faces
        let grid = GridPart(boxes: [.init(lo: SIMD3(-20, -20, 20), hi: SIMD3(20, 20, 24), n: 297),
                                    .init(lo: SIMD3(-5, -5, 0), hi: SIMD3(5, 5, 20), n: 80),
                                    .init(lo: SIMD3(30, -15, 0), hi: SIMD3(40, 15, 100), n: 50),
                                    .init(lo: SIMD3(-40, -15, 0), hi: SIMD3(-30, 15, 100), n: 50)])
        #expect(grid.faces > 1_190_000)
        let file = try grid.write()
        defer { Scratch.remove(file) }
        let (window, draw, kit) = try await drawWindow(sway: true, file: file)
        defer { withExtendedLifetime(window) {} }
        let build = try #require(window.wiring?.build)
        try await eventually("Draw's build (the pad only)", tries: 1500) { build.latest != nil && !build.building }
        let beat = Heartbeat()
        var worstCall = 0.0, worstBeat = 0.0
        func timed(_ what: String, _ p: ViewerController.HandPointer) async throws {
            beat.start()
            let t0 = ContinuousClock.now
            draw.pointerClicked(p)
            let call = Heartbeat.ms(ContinuousClock.now - t0)
            try await eventually(what, tries: 1500) { !draw.clickPending }
            let gap = beat.stop()
            print(String(format: "C-08 click %@: pointerClicked %.2f ms, main-thread longest job %.2f ms; walls %d, message '%@'", what, call, gap,
                         window.session.drawnWalls.count, draw.message))
            worstCall = max(worstCall, call); worstBeat = max(worstBeat, gap)
            try await eventually("the sweep after \(what)", tries: 1500) { draw.sweepCurrent || window.session.drawnWalls.isEmpty }
        }
        // a sway brace on the post's outer side (the first: it builds the triangle index)
        try await timed("first brace", try grid.pointer(kit, box: 2, side: 3, at: SIMD3(40, 0, 30), origin: SIMD3(90, 0, 30)))
        #expect(window.session.drawnWalls.count == 1 && window.session.drawnWalls.first?.kind == .sway, "the brace stands")
        // a wall under the slab, across the overhang
        try await timed("wall start", try grid.pointer(kit, box: 0, side: 0, at: SIMD3(-12, 12, 20), origin: SIMD3(-12, 12, -50)))
        try await timed("wall end", try grid.pointer(kit, box: 0, side: 0, at: SIMD3(12, 12, 20), origin: SIMD3(12, 12, -50)))
        #expect(window.session.drawnWalls.count == 2 && draw.message.isEmpty, "the wall stands: \(draw.message)")
        for w in window.session.drawnWalls {     // the clicks' results, to compare across a change
            let o = draw.outcomes[w.id]
            print("C-08 result \(w.kind) a \(w.a) b \(w.b) m \(String(describing: w.m)) face \(w.face) capped \(w.capped)",
                  "ok \(String(describing: o?.ok)) tines \(String(describing: o?.tines)) height \(String(describing: o?.height)) length \(String(describing: o?.length))")
        }
        #expect(worstCall < 16, "a click held the main thread for \(worstCall) ms")
        #expect(worstBeat < 16, "a click's answer held the main thread for \(worstBeat) ms")
    }
}
