import SwiftUI
import Testing
@testable import Support_Fins

/// Fake features that only record what they were asked, to pin how the window and the menu bar compose them.
@MainActor private enum Probes {
    struct Config {
        var overlays: [Int] = []
        var escape: [(priority: Int, takes: Bool)] = []
        var statusOrder: Int?
        var slotOrder: Int?
        var menu: [(MenuID, order: Int, titles: [String])] = []
    }
    static var config: [String: Config] = [:]
    static var log: [String] = []
}

@MainActor private protocol ProbeFeature: Feature { var host: ViewerModel { get } }
extension ProbeFeature {
    var name: String { String(describing: Self.self) }
    private var cfg: Probes.Config { Probes.config[name] ?? Probes.Config() }
    var overlays: [WindowOverlay] { cfg.overlays.map { o in WindowOverlay("\(name)\(o)", order: o) { EmptyView() } } }
    var escapeHandlers: [EscapeHandler] {
        cfg.escape.enumerated().map { i, e in
            EscapeHandler(id: "\(name)\(i)", priority: e.priority) { Probes.log.append("\(self.name)\(i)"); return e.takes }
        }
    }
    var statusLines: [StatusLine] { cfg.statusOrder.map { [StatusLine(name, order: $0, text: name)] } ?? [] }
    var toolbarSlots: [ToolbarSlot] {
        cfg.slotOrder.map { [ToolbarSlot(id: name, title: "T", systemImage: "circle", help: "H", order: $0)] } ?? []
    }
    var inspectorSections: [InspectorSection] { [InspectorSection(name, slot: .basics, order: cfg.statusOrder ?? 0) { EmptyView() }] }
    static func menus(_ instance: Self?) -> [MenuSection] {
        (Probes.config[String(describing: Self.self)]?.menu ?? []).map { m in
            MenuSection(menu: m.0, order: m.order, entries: m.titles.map { t in .item(t) {} })
        }
    }
}
@MainActor private final class PA: ProbeFeature { unowned let host: ViewerModel; init(host: ViewerModel) { self.host = host } }
@MainActor private final class PB: ProbeFeature { unowned let host: ViewerModel; init(host: ViewerModel) { self.host = host } }
@MainActor private final class PC: ProbeFeature { unowned let host: ViewerModel; init(host: ViewerModel) { self.host = host } }

@MainActor
private func probes(_ a: Probes.Config = .init(), _ b: Probes.Config = .init(), _ c: Probes.Config = .init()) -> FeatureSet {
    Probes.config = ["PA": a, "PB": b, "PC": c]
    Probes.log = []
    return FeatureSet(types: [PA.self, PB.self, PC.self], host: ViewerModel())
}

@MainActor
struct FeatureSetTests {
    @Test func overlaysStackByOrderAndTiesKeepTheFeatureListOrder() {
        let set = probes(.init(overlays: [30, 10]), .init(overlays: [20, 10]), .init(overlays: [10]))
        #expect(set.overlays.map(\.id) == ["PA10", "PB10", "PC10", "PB20", "PA30"])
    }

    @Test func theEscCascadeRunsHighestPriorityFirstAndStopsAtTheFirstThatTakesIt() {
        let set = probes(.init(escape: [(50, true)]), .init(escape: [(100, false), (100, true)]), .init(escape: [(100, true)]))
        #expect(set.escape())
        #expect(Probes.log == ["PB0", "PB1"])            // PB1 took it: PC (same priority, later) and PA (lower) never ran
        Probes.log = []
        let none = probes(.init(escape: [(50, false)]), .init(escape: [(100, false)]))
        #expect(!none.escape())
        #expect(Probes.log == ["PB0", "PA0"])
    }

    @Test func statusLinesSlotsAndInspectorSectionsComposeInOrder() {
        let set = probes(.init(statusOrder: 20, slotOrder: 20), .init(statusOrder: 10, slotOrder: 10), .init(slotOrder: 15))
        #expect(set.statusLines.map(\.text) == ["PB", "PA"])
        #expect(set.toolbarSlots.map(\.id) == ["PB", "PC", "PA"])
        #expect(set.inspectorSections(.basics).map(\.id) == ["PC", "PB", "PA"])
        #expect(set.inspectorSections(.top).isEmpty)
    }

    @Test func menuSectionsInterleaveByOrderAcrossFeaturesAndMenusByTheirOrder() {
        let types: [any Feature.Type] = [PA.self, PB.self, PC.self]
        Probes.config = [
            "PA": .init(menu: [(.model, 20, ["a1"]), (.view, 0, ["a0"])]),
            "PB": .init(menu: [(.model, 10, ["b1", "b2"])]),
            "PC": .init(menu: [(MenuID(title: "Plates", order: 5), 0, ["c"]), (.model, 10, ["c1"])]),
        ]
        #expect(FeatureSet.menuSnapshot(types: types, in: nil) == ["Plates", "  c", "View", "  a0", "Model", "  b1", "  b2", "  c1", "  a1"])
    }

    @Test func eachSetBuildsItsOwnInstancesAndFindsThemByType() {
        let a = probes(), b = probes()
        #expect(a[PA.self] != nil && a[PA.self] !== b[PA.self] && a[RotateFeature.self] == nil)
    }

    // MARK: the real features

    @Test func theWindowsOverlaysStackAsTheyAlwaysHave() throws {
        let model = ViewerModel()
        var expected = ["empty-state", "status", "build-spinner", "lay-face-hint", "view-cube-and-toolbar"]
        #if DEBUG
        expected.insert("frame-time", at: 0)
        #endif
        #expect(model.features.overlays.map(\.id) == expected)
        #expect(model.toolbar.slots.map(\.id) == ["rotate", "suggest", "strength", "view"])
        #expect(model.features.windowToolbarItems.map(\.id) == ["export"])
        #expect(model.features.inspectorSections(.top).map(\.id) == ["readout"])
        #expect(model.features.inspectorSections(.basics).map(\.id) == ["remove-fins"])
    }

    @Test func theEscCascadePutsAnArmedPointerModeBeforeEndingRemoveFins() {
        #expect(ViewerModel().features.escapeHandlers.map(\.id) == ["lay-face", "remove-fins"])
    }

    @Test func theRealFeaturesRegisterTheirSlotsOnlyWhenTheyCanOpen() {
        let model = ViewerModel()
        #expect(model.toolbar.badges[.rotate] != nil && model.toolbar.badges[.view] != nil)
        #expect(model.toolbar.badges[.suggest] == nil && model.toolbar.badges[.strength] == nil)   // no part yet
    }

    /// The menu bar, as it has always been: View, Model, then File's Export. Keep in step with docs/UI-MAP.md.
    @Test func theMenuBarKeepsItsItemsAndTheirOrder() {
        var view = ["View", "  Show Rotate Rings", "  Rebuild While Dragging", "  Frame Part [0]", "  Home", "  Show Print Layers"]
        #if DEBUG
        view.append("  Show Frame Time")
        #endif
        view += ["  -", "  Front [1]", "  Back [2]", "  Left [3]", "  Right [4]", "  Top [5]", "  Bottom [6]"]
        let model = ["Model", "  Rotate 90° About X", "  Rotate 90° About Y", "  Rotate 90° About Z", "  Reset Rotation", "  -",
                     "  Lay a Face Flat", "  -", "  Suggest Orientation", "  Strength Arrow…"]
        let file = ["File", "  Export", "  > STL", "  > 3MF", "  > Fins only", "  Export Again [e]"]
        // no window focused
        #expect(FeatureSet.menuSnapshot(in: nil) == view + model + file)
        // a window with nothing to export: the same items (the note under Export appears once there is a build)
        #expect(FeatureSet.menuSnapshot(in: ViewerModel().features) == view + model + file)
    }
}
