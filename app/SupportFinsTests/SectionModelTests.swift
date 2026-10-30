import Foundation
import SwiftUI
import Testing
import Viewer
import simd
@testable import Support_Fins

/// Section view (`web/ui/section.js`) on the app side: the tool badge's click rules (`web/ui/toolbar.js`), Ghost /
/// Hide remembered as `sf.sectionGhost`, and the cut going away with the part.
@MainActor
struct SectionModelTests {
    func defaults() -> UserDefaults {
        let name = "sf-section-test-\(UUID().uuidString)"
        let d = UserDefaults(suiteName: name)!
        d.removePersistentDomain(forName: name)
        return d
    }

    func window() throws -> (ViewerModel, SectionFeature, SectionTool) {
        let host = ViewerModel()
        let feature = try #require(host.features[SectionFeature.self])
        let controller = try #require(host.controller)
        let cube = GeneratedMesh.cube(edge: 40, device: controller.renderer.device)
        controller.show([cube], name: "cube")
        return (host, feature, controller.section)
    }

    @Test func ghostIsTheDefaultAndHideIsRemembered() throws {
        let d = defaults()
        let r = try #require(ViewerRenderer())
        let tool = r.section
        let model = SectionModel(tool: tool, defaults: d)
        #expect(model.ghost)
        model.setGhost(false)
        #expect(!tool.ghost)
        #expect(d.bool(forKey: "sf.sectionGhost") == false && d.object(forKey: "sf.sectionGhost") != nil)
        // a new window starts from what was stored
        let again = SectionModel(tool: ViewerRenderer()!.section, defaults: d)
        #expect(!again.ghost)
        again.setGhost(true)
        #expect(SectionModel(tool: ViewerRenderer()!.section, defaults: d).ghost)
    }

    @Test func theBadgeTurnsTheToolOnAndOpensItsPopoverThenOffFromTheOpenPopover() throws {
        let (host, _, tool) = try window()
        let bar = host.toolbar
        #expect(!tool.isOn && bar.open == nil)
        bar.toggle(.section)                       // off -> on, popover open
        #expect(tool.isOn && bar.open == .section)
        bar.toggle(.rotate)                        // another popover: the tool stays on
        #expect(tool.isOn && bar.open == .rotate)
        bar.toggle(.section)                       // the lit badge brings its popover back
        #expect(tool.isOn && bar.open == .section)
        bar.toggle(.section)                       // with its popover open: off
        #expect(!tool.isOn && bar.open == nil)
    }

    @Test func foldingThePopoverLeavesTheCutOn() throws {
        let (host, _, tool) = try window()
        host.toolbar.toggle(.section)
        host.toolbar.fold()
        #expect(tool.isOn && host.toolbar.open == nil)
        #expect(host.toolbar.badges[.section]?.isLit() == true)
    }

    @Test func noPartMeansNoCut() throws {
        let host = ViewerModel()
        host.toolbar.toggle(.section)
        #expect(host.controller?.section.isOn == false)
        #expect(host.toolbar.open == nil)
    }

    @Test func aNewPartClearsTheCutAndFoldsItsPopover() throws {
        let (host, feature, tool) = try window()
        host.toolbar.toggle(.section)
        #expect(tool.isOn)
        feature.partWillShow()
        #expect(!tool.isOn && host.toolbar.open == nil)
    }

    @Test func theViewMenuHasATogglePerTheWeb() throws {
        let (host, feature, tool) = try window()
        defer { withExtendedLifetime(host) {} }
        let sections = SectionFeature.menus(feature)
        guard case .toggle(let item) = sections[0].entries[1] else { Issue.record("no toggle"); return }
        #expect(item.title == "Section View" && item.enabled)
        item.isOn.wrappedValue = true
        #expect(tool.isOn)
        #expect(item.isOn.wrappedValue)
    }

    @Test func switchingTheCutOffFromTheMenuFoldsItsPopover() throws {
        let (host, feature, tool) = try window()
        defer { withExtendedLifetime(host) {} }
        host.toolbar.toggle(.section)
        guard case .toggle(let item) = SectionFeature.menus(feature)[0].entries[1] else { Issue.record("no toggle"); return }
        item.isOn.wrappedValue = false
        #expect(!tool.isOn && host.toolbar.open == nil)
        item.isOn.wrappedValue = true                 // back on from the menu: the popover stays folded
        #expect(tool.isOn && host.toolbar.open == nil)
    }

    /// A rescale reloads the part (`ui/scale.js setScale` -> `setPart` -> `resetPartState` -> `setSection(false)`,
    /// `ui/part.js:157`): the cut goes off and its popover folds, as for a new file.
    @Test func aRescaleSwitchesTheCutOff() async throws {
        let host = ViewerModel()
        let controller = try #require(host.controller, "no Metal device")
        let stl = URL(fileURLWithPath: try TestParts.write(TestParts.box(-10, 10, -5, 5, 0, 20), name: "section-\(UUID().uuidString).stl"))
        await host.load(url: stl)
        let scale = try #require(host.features[ScaleFeature.self])
        host.toolbar.toggle(.section)
        #expect(controller.section.isOn && host.toolbar.open == .section)
        scale.model.commit(text: "150")
        for _ in 0..<1000 where scale.model.scale != 1.5 { try await Task.sleep(for: .milliseconds(10)) }
        #expect(scale.model.scale == 1.5, "the reload")
        #expect(!controller.section.isOn && host.toolbar.open == nil)
    }

    @Test func tiltingAndPushingTheCutNeverTouchesThePose() throws {
        let (host, _, tool) = try window()
        let pose = host.session.pose
        host.toolbar.toggle(.section)
        tool.flip()
        tool.scrollPush(5)
        tool.aimAtView()
        #expect(host.session.pose == pose && !host.session.canUndo)    // view only: not undoable, not in the pose
    }

    // MARK: a parked model keeps its cut (`sectionState` / `restoreSection`, `ui/models.js:68,104`)

    /// A window with a model on show and its plate board (`PlateBoardTests`), the cut on and moved off its aim.
    func cutPlate(_ name: String) async throws -> (ViewerModel, PlateBoard, SectionTool, simd_float4x4) {
        let (host, board) = try await PlateBoardTests().window(name: name)
        let tool = try #require(host.controller?.section)
        host.toolbar.toggle(.section)
        tool.flip()
        tool.scrollPush(3)
        let held = try #require(tool.heldCut)
        return (host, board, tool, held)
    }

    @Test func aPlateLeftWithTheCutOnHasItBackWhereItWasOnTheModel() async throws {
        let (host, board, tool, held) = try await cutPlate("cut-a")
        let plane = tool.plane
        board.addPlate()
        // parking takes the part off the page, and the cut with it (`stashPart` -> `resetPartState` -> `setSection(false)`)
        #expect(!tool.isOn && host.toolbar.open == nil)
        await host.load(url: try PlateBoardTests().stl("cut-b", slab: 20))
        #expect(!tool.isOn, "the other plate's model has no cut")
        board.switchPlate(0)
        #expect(tool.isOn && tool.heldCut == held)
        #expect(simd_distance(tool.plane.position, plane.position) < 1e-4 && simd_distance(tool.plane.normal, plane.normal) < 1e-5)
        // switched on as `setSection(true)` does: the badge lit, its popover not opened
        #expect(host.toolbar.badges[.section]?.isLit() == true && host.toolbar.open == nil)
        board.switchPlate(1)
        #expect(!tool.isOn)
        board.switchPlate(0)
        #expect(tool.isOn && tool.heldCut == held, "parked again with the cut it came back with")
    }

    @Test func aPlateLeftWithTheCutOffComesBackWithItOff() async throws {
        let (host, board, tool, _) = try await cutPlate("cut-off-a")
        tool.setOn(false)
        board.addPlate()
        await host.load(url: try PlateBoardTests().stl("cut-off-b", slab: 20))
        host.toolbar.toggle(.section)
        #expect(tool.isOn)
        board.switchPlate(0)
        #expect(!tool.isOn && tool.heldCut == nil)
    }

    @Test func aModelSwappedOutOnItsPlateKeepsItsCut() async throws {
        let (host, board, tool, held) = try await cutPlate("cut-swap-a")
        try await PlateBoardTests().add(host, "cut-swap-b")
        #expect(board.plate.live == 1 && !tool.isOn)
        board.swap(to: 0)
        #expect(board.plate.live == 0 && tool.isOn && tool.heldCut == held)
        board.swap(to: 1)
        #expect(!tool.isOn)
    }
}
