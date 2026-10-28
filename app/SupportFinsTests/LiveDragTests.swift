import Foundation
import Testing
import Viewer
import simd

@testable import Support_Fins

/// The scheduler's policy for a live rebuild while a ring is dragged, against a driver the test lands by hand.
@MainActor
@Suite("Live drag: scheduler")
struct LiveDragSchedulerTests {
    final class Pose { var rotation: [Double] = [1, 0, 0, 0, 1, 0, 0, 0, 1] }

    func rig() -> (BuildController, FakeDriver, ManualClock, Pose) {
        let clock = ManualClock(), driver = FakeDriver(), pose = Pose()
        let controller = BuildController(
            driver: driver, request: { BuildRequest(rotation: pose.rotation, optionsJSON: "{}") },
            sleep: { try await clock.sleep($0) })
        return (controller, driver, clock, pose)
    }
    func turn(_ pose: Pose, _ k: Double) { pose.rotation = [1, 0, 0, 0, k, 0, 0, 0, 1] }

    @Test func aNewPoseIsBuiltAtOnceWithNoDebounce() async {
        let (c, d, _, pose) = rig()
        c.setEnabled(true)
        d.land()
        turn(pose, 0.9)
        c.refreshLive()
        #expect(d.generation == 2 && d.requests.last?.rotation == pose.rotation, "no 180 ms wait")
    }

    @Test func aNewerPoseSupersedesTheBuildInFlight() async {
        let (c, d, clock, pose) = rig()
        c.setEnabled(true)
        d.land()
        for k in [0.9, 0.8, 0.7] {
            turn(pose, k)
            c.refreshLive()
            await clock.advance(.milliseconds(3))
        }
        #expect(d.generation == 4, "each pose started a build; the engine stops the stale ones")
        #expect(d.requests.last?.rotation == pose.rotation)
        #expect(!d.calls.contains("cancel"), "a newer build supersedes; nothing is dropped")
    }

    @Test func aBuildThatHasRunLongEnoughLandsAndTheNewestPoseIsBuiltNext() async {
        let (c, d, clock, pose) = rig()
        c.setEnabled(true)
        d.land()
        turn(pose, 0.9)
        c.refreshLive()                                       // generation 2
        await clock.advance(BuildController.liveLandAfter)
        turn(pose, 0.8)
        c.refreshLive()
        turn(pose, 0.7)
        c.refreshLive()
        #expect(d.generation == 2, "the slow build is let land, not superseded forever")
        d.land()
        await clock.settleTasks()
        #expect(d.generation == 3 && d.requests.last?.rotation == pose.rotation, "then the newest pose, once")
    }

    @Test func aBuildPastTheLandingTimeIsNotSupersededByEveryPose() async {
        let (c, d, clock, pose) = rig()
        c.setEnabled(true)
        d.land()
        turn(pose, 0.9)
        c.refreshLive()
        await clock.advance(.milliseconds(9))
        turn(pose, 0.8)
        c.refreshLive()
        #expect(d.generation == 2, "9 ms in: it lands, the pose waits")
        #expect(BuildController.liveLandAfter == .milliseconds(8))
    }

    @Test func nothingIsBuiltWhileTheFinsAreOff() async {
        let (c, d, _, pose) = rig()
        turn(pose, 0.9)
        c.refreshLive()
        #expect(d.generation == 0)
    }

    @Test func theReleaseBuildsTheFinalPoseAtOnce() async {
        let (c, d, clock, pose) = rig()
        c.setEnabled(true)
        d.land()
        turn(pose, 0.9)
        c.refreshLive()
        d.land()
        turn(pose, 0.8)
        c.endLive()
        #expect(d.generation == 3 && d.requests.last?.rotation == pose.rotation, "at once, not after the debounce")
        _ = clock
    }

    @Test func theReleaseKeepsTheLiveBuildThatAlreadyIsTheFinalOne() async {
        let (c, d, _, pose) = rig()
        c.setEnabled(true)
        d.land()
        turn(pose, 0.9)
        c.refreshLive()                                       // in flight, for the pose the ring is released at
        c.endLive()
        #expect(d.generation == 2 && d.building)
    }

    @Test func aLiveChainStopsAtTheRelease() async {
        let (c, d, clock, pose) = rig()
        c.setEnabled(true)
        d.land()
        turn(pose, 0.9)
        c.refreshLive()
        await clock.advance(BuildController.liveLandAfter)
        turn(pose, 0.8)
        c.refreshLive()                                       // waits for the slow build
        c.endLive()                                           // the final build supersedes it
        let g = d.generation
        d.land()
        await clock.settleTasks()
        #expect(d.generation == g, "the landing does not start one more live build")
    }
}

/// The window: the real engine on a small part, a ring dragged through `RingEdits`.
@MainActor
@Suite("Live drag: window")
struct LiveDragWindowTests {
    struct Rig {
        let w: Window
        let edits: RingEdits
        let live: Flag
    }
    final class Flag { var on = true }

    static func open(live: Bool = true) async throws -> Rig {
        let flag = Flag()
        flag.on = live
        let controller = try #require(ViewerController(), "no Metal device")
        let session = PartSession()
        let clock = ManualClock()
        let wiring = BuildWiring(session: session, controller: controller, liveDrag: { flag.on },
                                 sleep: { try await clock.sleep($0) })
        let loaded = try await CoreLoader().load(url: try tURL(), device: controller.renderer.device)
        session.replaceParts(loaded.compactMap { $0.mesh as? CorePart })
        wiring.attach()
        controller.show(loaded.map(\.mesh), name: "bracket.stl")
        let w = Window(session: session, controller: controller, wiring: wiring, clock: clock)
        try await w.finsOn()
        return Rig(w: w, edits: RingEdits(session: session, controller: controller), live: flag)
    }

    func x(_ degrees: Double) -> simd_double3x3 { RotateGizmo.rotation(axis: [1, 0, 0], radians: degrees * .pi / 180) }
    func turn(_ r: Rig, _ degrees: Double) {
        r.edits.handle(.turned(delta: x(degrees), degrees: degrees, handle: .axis(.x)))
    }

    @Test func finsAreBuiltDuringTheDragWithoutWaitingForTheRelease() async throws {
        let r = try await Self.open()
        let w = r.w
        let before = w.build.generation
        r.edits.handle(.began(.axis(.x)))
        turn(r, 20)
        try await w.eventually("a build for the pose mid-drag") { w.build.generation == before + 1 }
        try await w.eventually("it lands") { !w.build.building && w.build.latestRotation == w.session.pose.columnMajor }
        #expect(w.session.ringDragging && w.controller.renderer.dimSupports == false, "current pose: not pale")
        turn(r, 35)
        try await w.eventually("the next pose is built too") { w.build.generation == before + 2 }
        try await w.eventually("it lands") { !w.build.building && w.build.latestRotation == w.session.pose.columnMajor }
        r.edits.handle(.ended(turned: true))
        #expect(w.session.history.undoStack.count == 2, "one step for the drag (plus Add Fins)")
    }

    @Test func aStalePoseIsSupersededAndOnlyTheNewestLands() async throws {
        let r = try await Self.open()
        let w = r.w
        let before = w.build.generation
        r.edits.handle(.began(.axis(.x)))
        turn(r, 20)
        try await w.eventually("first build started") { w.build.generation == before + 1 }
        turn(r, 40)                               // before the first can land, or just after: either way the newest wins
        try await w.eventually("second build started") { w.build.generation >= before + 2 }
        try await w.eventually("it lands") { !w.build.building }
        #expect(w.build.latestRotation == w.session.pose.columnMajor)
        r.edits.handle(.ended(turned: true))
    }

    @Test func theFinsAreOnlyPaleWhileTheyLagThePose() async throws {
        let r = try await Self.open()
        let w = r.w
        r.edits.handle(.began(.axis(.x)))
        turn(r, 30)
        try await w.eventually("the build starts") { w.build.building }
        #expect(w.controller.renderer.dimSupports, "lagging the pose: pale")
        try await w.eventually("it lands") { !w.build.building }
        #expect(!w.controller.renderer.dimSupports)
        r.edits.handle(.ended(turned: true))
    }

    @Test func theReleaseDoesTheFinalBuildAtTheReleasedPose() async throws {
        let r = try await Self.open()
        let w = r.w
        r.edits.handle(.began(.axis(.x)))
        turn(r, 30)
        turn(r, 60)
        r.edits.handle(.ended(turned: true))
        try await w.eventually("the final build lands") { !w.build.building && w.build.latestRotation == w.session.pose.columnMajor }
        #expect(!w.session.ringDragging && !w.controller.renderer.dimSupports)
        #expect(w.session.history.undoStack.count == 2)
    }

    @Test func withRebuildWhileDraggingOffNothingIsBuiltUntilTheRelease() async throws {
        let r = try await Self.open(live: false)
        let w = r.w
        let before = w.build.generation
        r.edits.handle(.began(.axis(.x)))
        turn(r, 30)
        turn(r, 60)
        await w.clock.advance(.seconds(1))
        try await Task.sleep(for: .milliseconds(150))
        #expect(w.build.generation == before, "the web: the fins wait for the drag to end")
        r.edits.handle(.ended(turned: true))
        try await w.eventually("the release builds, at once") { w.build.generation == before + 1 }
        try await w.eventually("it lands") { !w.build.building && w.build.latestRotation == w.session.pose.columnMajor }
    }

    @Test func aTurnByAButtonStillWaitsForTheDebounce() async throws {
        let r = try await Self.open()
        let w = r.w
        let before = w.build.generation
        w.session.rotate90(about: .x)
        try await w.eventually("the change is seen") { w.session.pose != .identity }
        await w.clock.advance(.milliseconds(179))
        #expect(w.build.generation == before)
        await w.clock.advance(.milliseconds(1))
        try await w.eventually("the rebuild") { w.build.generation == before + 1 }
    }
}
