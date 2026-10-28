import Foundation
import Metal
import Testing
import Viewer
import simd
@testable import Support_Fins

@MainActor
struct RotateModelTests {
    func rig() throws -> (PartSession, RotateModel, ToolbarRegistry) {
        let device = try #require(MTLCreateSystemDefaultDevice(), "no Metal device")
        let part = try #require(CorePart(part: try TestParts.cube(), device: device))
        let s = PartSession(parts: [part])
        let r = ToolbarRegistry()
        let m = RotateModel(session: s, defaults: UserDefaults(suiteName: "RotateModelTests.\(UUID())")!)
        m.registry = r
        return (s, m, r)
    }

    /// Each expected string is `showDelta`'s template run in node on the same numbers (`ui/pose.js:106`).
    @Test(arguments: [
        ("X", 90.0, "X +90°"), ("Z", -90.0, "Z -90°"), ("Y", 5.0, "Y +5°"), ("XYZE", 12.5, "free +12.5°"),
        ("XYZE", -7.25, "free -7.3°"), ("E", 33.0, "E +33°"), ("X", -0.5, "X -0.5°"),
    ])
    func deltaWording(row: (String, Double, String)) {
        #expect(RotateModel.deltaText(name: row.0, degrees: row.1) == row.2)
    }

    @Test func aZeroTurnSaysNothing() { #expect(RotateModel.deltaText(name: "X", degrees: 0) == nil) }

    @Test func each90ButtonIsOneUndoStepAndSetsTheDelta() throws {
        let (s, m, _) = try rig()
        m.rotate90(about: .x)
        #expect(m.delta == "X +90°" && m.now == "X 90° · Y 0° · Z 0°")
        m.rotate90(about: .z)
        #expect(s.history.undoStack.count == 2)
        s.undo()
        #expect(s.pose.isSame(as: Pose.identity.rotated90(about: .x)))
        s.undo()
        #expect(s.pose == .identity)
    }

    @Test func resetIsOneStepAndClearsTheDelta() throws {
        let (s, m, _) = try rig()
        m.rotate90(about: .y)
        m.reset()
        #expect(s.pose == .identity && m.delta == "" && s.history.undoStack.count == 2)
        s.undo()
        #expect(s.pose.isSame(as: Pose.identity.rotated90(about: .y)))
    }

    @Test func resetOnAnUnturnedPartRecordsNothing() throws {
        let (s, m, _) = try rig()
        m.reset()
        #expect(!s.canUndo)
    }

    @Test func aRingDragFlashesTheAngleAndFadesAfterRelease() throws {
        let (_, m, r) = try rig()
        m.ring(.began(.axis(.z)))
        m.ring(.turned(delta: matrix_identity_double3x3, degrees: 45, handle: .axis(.z)))
        #expect(m.delta == "Z +45°" && r.flash?.slot == .rotate)
        #expect(r.flash?.text.hasPrefix("Z +45°") == true)
        m.ring(.ended(turned: true))
        #expect(m.delta == "" && r.flash?.text.hasPrefix("Z +45°") == true)   // the last delta stays on the badge
    }

    @Test func noFlashWhileThePopoverIsOpen() throws {
        let (_, m, r) = try rig()
        r.open = .rotate
        m.ring(.began(.free))
        m.ring(.turned(delta: matrix_identity_double3x3, degrees: 10, handle: .free))
        #expect(r.flash == nil && m.delta == "free +10°")
    }

    @Test func showRingsIsRememberedAndReachesTheViewer() throws {
        let d = UserDefaults(suiteName: "RotateModelTests.\(UUID())")!
        let c = try #require(ViewerController(), "no Metal device")
        let m = RotateModel(session: PartSession(), controller: c, defaults: d)
        #expect(m.showRings && c.showRings)
        m.showRings = false
        #expect(!c.showRings)
        #expect(!RotateModel(session: PartSession(), defaults: d).showRings)
    }
}
