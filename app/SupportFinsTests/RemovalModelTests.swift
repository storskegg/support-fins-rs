import Foundation
import Metal
import SupportFinsCore
import Testing
import Viewer

@testable import Support_Fins

/// A T whose slab overhangs on every side, so there are several fins to remove one by one.
private func tPart() throws -> Part {
    let path = try TestParts.write(TestParts.box(-5, 5, -5, 5, 0, 20) + TestParts.box(-30, 30, -30, 30, 20, 24),
                                   to: Scratch.file("removal-tests-t.stl"))
    return try #require(try loadParts(path: path).first)
}

private let identity: [Double] = [1, 0, 0, 0, 1, 0, 0, 0, 1]

@MainActor
@Suite("RemovalModel")
struct RemovalModelTests {
    final class Env { var finsOn = true; var mode = FinSettings.Mode.auto }

    func rig() async throws -> (RemovalModel, BuildModel, Env) {
        let build = BuildModel(part: try tPart())
        let env = Env()
        build.build(rotation: identity, optionsJSON: "{}")
        await build.settled()
        #expect(build.error == nil)
        return (RemovalModel(build: build, finsOn: { env.finsOn }, mode: { env.mode }), build, env)
    }

    @Test func theControlsAreOfferedOnlyInAutoAndFullWithFinsOn() async throws {
        let (m, _, env) = try await rig()
        #expect(m.controlsShown && m.autoLike)
        env.mode = .full
        #expect(m.controlsShown)
        env.mode = .draw
        #expect(!m.controlsShown)
        m.arm()
        #expect(!m.armed, "Draw places walls by hand")
        env.mode = .auto
        env.finsOn = false
        m.arm()
        #expect(!m.armed && !m.controlsShown)
    }

    @Test func armingTogglesAndAnythingThatEndsTheModeDisarmsIt() async throws {
        let (m, _, env) = try await rig()
        #expect(m.toggleLabel == "Remove fins" && !m.isActive)
        m.toggle()
        #expect(m.armed && m.isActive && m.toggleLabel == "Click a fin — Esc done")
        m.toggle()
        #expect(!m.armed)
        m.arm()
        env.finsOn = false                          // the fins toggle went off
        #expect(!m.isActive, "armed but not active")
        m.sync()
        #expect(!m.armed)
        env.finsOn = true
        m.arm()
        env.mode = .draw                            // a mode switch
        m.sync()
        #expect(!m.armed)
    }

    @Test func aTriangleBelongsToTheFinWhoseRangeHoldsIt() async throws {
        let (m, build, _) = try await rig()
        let fins = try #require(build.records?.fins)
        #expect(fins.count > 1)
        var total = 0
        for fin in fins {
            for r in fin.triRanges {
                #expect(m.fin(atTriangle: r.from / 3)?.id == fin.id)
                #expect(m.fin(atTriangle: (r.to - 3) / 3)?.id == fin.id)
                total += r.count / 3
            }
        }
        #expect(total > 0)
        let vertices = Int(build.latest!.fins().count())
        #expect(m.fin(atTriangle: vertices / 3) == nil && m.fin(atTriangle: -1) == nil)
    }

    @Test func hoverLightsTheFinAClickWouldDrop() async throws {
        let (m, build, _) = try await rig()
        let fin = try #require(build.records?.fins.first)
        let tri = fin.triRanges[0].from / 3
        m.hover(triangle: tri)
        #expect(m.hoveredFin == nil, "only while armed")
        m.arm()
        m.hover(triangle: tri)
        #expect(m.hoveredFin == fin.id && m.highlightedRanges == build.ranges(ofFin: fin.id))
        m.hover(triangle: nil)
        #expect(m.hoveredFin == nil && m.highlightedRanges.isEmpty)
    }

    @Test func aClickRemovesJustThatFinAndTellsTheSession() async throws {
        let (m, build, _) = try await rig()
        var changes: [(Int, Int)] = []
        m.onChange = { old, new in changes.append((old.count, new.count)) }
        m.arm()
        let fin = try #require(build.records?.fins.first)
        m.click(triangle: nil)
        m.click(triangle: 1_000_000)
        #expect(m.removed.isEmpty && changes.isEmpty, "empty space and nothing in range")
        m.click(triangle: fin.triRanges[0].from / 3)
        #expect(m.removed.count == 1 && changes.count == 1 && changes[0] == (0, 1))
        #expect(m.hiddenRanges == build.ranges(ofFin: fin.id))
        #expect(m.removedHere.keys.contains(fin.id) && m.removedHere.count == 1)
        // every other fin is still drawn
        let drawn = SupportSegments.fins(total: Int(build.latest!.fins().count()), hidden: m.hiddenRanges, crest: []).reduce(0) { $0 + $1.range.count }
        #expect(drawn == Int(build.latest!.fins().count()) - build.ranges(ofFin: fin.id).reduce(0) { $0 + $1.count })
    }

    @Test func aStoredKeyThatAlreadyNamesAFinGetsASuffixSoEachStaysUnique() async throws {
        let (m, build, _) = try await rig()
        m.arm()
        let tri = try #require(build.records?.fins.first).triRanges[0].from / 3
        m.click(triangle: tri)
        m.click(triangle: tri)          // the same fin again (a stale key read the same)
        #expect(m.removed.count == 2)
        #expect(m.removed.contains { $0.raw.hasSuffix("~1") })
    }

    @Test func aRemovalSurvivesARebuildOfTheSamePose() async throws {
        let (m, build, _) = try await rig()
        m.arm()
        let fin = try #require(build.records?.fins.first)
        m.click(triangle: fin.triRanges[0].from / 3)
        let before = m.hiddenRanges
        #expect(before == build.ranges(ofFin: fin.id))
        build.build(rotation: identity, optionsJSON: #"{"tineDensity":0.5}"#)
        await build.settled()
        let again = try #require(build.records?.fins.first { m.removedHere[$0.id] != nil })
        #expect(m.hiddenRanges == build.ranges(ofFin: again.id), "the key found its fin in the fresh build")
        #expect(m.removed.count == 1)
    }

    @Test func restoreAllBringsBackThisPosesFinsAndKeepsOtherPosesRemovals() async throws {
        let (m, build, _) = try await rig()
        let foreign = FinKey(raw: "prop:900,900,5:6:0")
        m.removed = [foreign]
        #expect(!m.canRestore, "a removal from another pose maps to nothing here")
        m.arm()
        for fin in build.records!.fins.prefix(2) { m.click(triangle: fin.triRanges[0].from / 3) }
        #expect(m.removed.count == 3 && m.canRestore && m.removedHere.count == 2)
        m.restoreAll()
        #expect(m.removed == [foreign] && m.hiddenRanges.isEmpty && !m.canRestore)
        m.restoreAll()                  // nothing to do
        #expect(m.removed == [foreign])
    }

    @Test func aNewPartForgetsTheKeysAndDisarms() async throws {
        let (m, build, _) = try await rig()
        m.arm()
        m.click(triangle: build.records!.fins[0].triRanges[0].from / 3)
        m.reset()
        #expect(m.removed.isEmpty && !m.armed && m.hoveredFin == nil)
    }

    @Test func theViewerFollowsThePointerClickAndEsc() async throws {
        let controller = try #require(ViewerController(), "no Metal device")
        let (m, build, _) = try await rig()
        build.show(on: controller.renderer)
        m.update(controller)
        #expect(!controller.supportPicking, "not armed")
        m.arm()
        m.update(controller)
        #expect(controller.supportPicking)

        let fin = try #require(build.records?.fins.first)
        let tri = fin.triRanges[0].from / 3
        controller.onSupportHover?(tri)
        #expect(controller.renderer.highlightedFinRanges == build.ranges(ofFin: fin.id))
        controller.onSupportClick?(tri)
        #expect(controller.renderer.hiddenFinRanges == build.ranges(ofFin: fin.id) && controller.renderer.highlightedFinRanges.isEmpty)
        controller.onCancelArmed?()
        #expect(!m.armed && !controller.supportPicking)
        // the fin stays removed after the mode ends
        #expect(controller.renderer.hiddenFinRanges == build.ranges(ofFin: fin.id))
    }
}
