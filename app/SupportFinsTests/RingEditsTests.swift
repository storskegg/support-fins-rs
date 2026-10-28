import Testing
import Viewer
import simd
@testable import Support_Fins

@MainActor
struct RingEditsTests {
    func rig() throws -> (PartSession, RingEdits) {
        let c = try #require(ViewerController(), "no Metal device")
        let s = PartSession()
        return (s, RingEdits(session: s, controller: c))
    }
    func z(_ degrees: Double) -> simd_double3x3 { RotateGizmo.rotation(axis: [0, 0, 1], radians: degrees * .pi / 180) }

    @Test func aDragIsOneUndoStepHoweverManyEventsItHad() throws {
        let (s, r) = try rig()
        r.handle(.began(.axis(.z)))
        for d in [5.0, 10, 45, 90] { r.handle(.turned(delta: z(d), degrees: d, handle: .axis(.z))) }
        #expect(!s.canUndo)                                   // nothing to undo while the pointer is down
        r.handle(.ended(turned: true))
        #expect(s.history.undoStack.count == 1)
        #expect(s.pose.isSame(as: Pose.identity.rotated(about: .z, degrees: 90)))
        s.undo()
        #expect(s.pose == .identity)
        s.redo()
        #expect(s.pose.isSame(as: Pose.identity.rotated(about: .z, degrees: 90)))
    }

    @Test func eachDragTurnsFromTheNewPose() throws {
        let (s, r) = try rig()
        for _ in 0..<2 {
            r.handle(.began(.screen))
            r.handle(.turned(delta: z(30), degrees: 30, handle: .screen))
            r.handle(.ended(turned: true))
        }
        #expect(s.pose.isSame(as: Pose.identity.rotated(about: .z, degrees: 60)))
        #expect(s.history.undoStack.count == 2)
    }

    @Test func aDragBackToWhereItStartedRecordsNothing() throws {
        let (s, r) = try rig()
        r.handle(.began(.free))
        r.handle(.turned(delta: z(20), degrees: 20, handle: .free))
        r.handle(.turned(delta: z(0), degrees: 0, handle: .free))
        r.handle(.ended(turned: false))
        #expect(s.pose == .identity && !s.canUndo)
    }
}
