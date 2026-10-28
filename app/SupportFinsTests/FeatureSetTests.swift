import SwiftUI
import Testing
@testable import Support_Fins

/// Fake features that only record what they were asked, to pin how the window and the menu bar compose them. What
/// each does comes from a task-local `Probes.state`, set for the length of one test (`withProbes`), so tests running
/// side by side never see each other's configuration or log.
private enum Probes {
    struct Config: Sendable {
        var overlays: [Int] = []
        var escape: [(priority: Int, takes: Bool)] = []
        var statusOrder: Int?
        var slotOrder: Int?
        var menu: [(MenuID, order: Int, titles: [String])] = []
    }
    struct State: Sendable {
        var config: [String: Config] = [:]
        var log = ProbeLog()
    }
    @TaskLocal static var state = State()
}

/// What the probes' Esc handlers ran, in order.
@MainActor private final class ProbeLog { var lines: [String] = [] }

@MainActor private protocol ProbeFeature: Feature { var host: ViewerModel { get } }
extension ProbeFeature {
    var name: String { String(describing: Self.self) }
    private var cfg: Probes.Config { Probes.state.config[name] ?? Probes.Config() }
    var overlays: [WindowOverlay] { cfg.overlays.map { o in WindowOverlay("\(name)\(o)", order: o) { EmptyView() } } }
    var escapeHandlers: [EscapeHandler] {
        let log = Probes.state.log
        return cfg.escape.enumerated().map { i, e in
            EscapeHandler(id: "\(name)\(i)", priority: e.priority) { log.lines.append("\(self.name)\(i)"); return e.takes }
        }
    }
    var statusLines: [StatusLine] { cfg.statusOrder.map { [StatusLine(name, order: $0, text: name)] } ?? [] }
    var toolbarSlots: [ToolbarSlot] {
        cfg.slotOrder.map { [ToolbarSlot(id: name, title: "T", systemImage: "circle", help: "H", order: $0)] } ?? []
    }
    var inspectorSections: [InspectorSection] { [InspectorSection(name, slot: .basics, order: cfg.statusOrder ?? 0) { EmptyView() }] }
    static func menus(_ instance: Self?) -> [MenuSection] {
        (Probes.state.config[String(describing: Self.self)]?.menu ?? []).map { m in
            MenuSection(menu: m.0, order: m.order, entries: m.titles.map { t in .item(t) {} })
        }
    }
}
@MainActor private final class PA: ProbeFeature { unowned let host: ViewerModel; init(host: ViewerModel) { self.host = host } }
@MainActor private final class PB: ProbeFeature { unowned let host: ViewerModel; init(host: ViewerModel) { self.host = host } }
@MainActor private final class PC: ProbeFeature { unowned let host: ViewerModel; init(host: ViewerModel) { self.host = host } }

private let probeTypes: [any Feature.Type] = [PA.self, PB.self, PC.self]

/// Runs `body` with the probes configured and a set of them on a window that lives as long as `body` (a set holds
/// its window weakly, as the window owns it).
@MainActor
private func withProbes<R>(_ a: Probes.Config = .init(), _ b: Probes.Config = .init(), _ c: Probes.Config = .init(),
                           _ body: (FeatureSet, ProbeLog) throws -> R) rethrows -> R {
    let state = Probes.State(config: ["PA": a, "PB": b, "PC": c])
    return try Probes.$state.withValue(state) {
        let window = ViewerModel()
        return try withExtendedLifetime(window) { try body(FeatureSet(types: probeTypes, host: window), state.log) }
    }
}

@MainActor
struct FeatureSetTests {
    @Test func overlaysStackByOrderAndTiesKeepTheFeatureListOrder() {
        withProbes(.init(overlays: [30, 10]), .init(overlays: [20, 10]), .init(overlays: [10])) { set, _ in
            #expect(set.overlays.map(\.id) == ["PA10", "PB10", "PC10", "PB20", "PA30"])
        }
    }

    @Test func theEscCascadeRunsHighestPriorityFirstAndStopsAtTheFirstThatTakesIt() {
        withProbes(.init(escape: [(50, true)]), .init(escape: [(100, false), (100, true)]), .init(escape: [(100, true)])) { set, log in
            #expect(set.escape())
            #expect(log.lines == ["PB0", "PB1"])            // PB1 took it: PC (same priority, later) and PA (lower) never ran
        }
        withProbes(.init(escape: [(50, false)]), .init(escape: [(100, false)])) { set, log in
            #expect(!set.escape())
            #expect(log.lines == ["PB0", "PA0"])
        }
    }

    @Test func statusLinesSlotsAndInspectorSectionsComposeInOrder() {
        withProbes(.init(statusOrder: 20, slotOrder: 20), .init(statusOrder: 10, slotOrder: 10), .init(slotOrder: 15)) { set, _ in
            #expect(set.statusLines.map(\.text) == ["PB", "PA"])
            #expect(set.toolbarSlots.map(\.id) == ["PB", "PC", "PA"])
            #expect(set.inspectorSections(.basics).map(\.id) == ["PC", "PB", "PA"])
            #expect(set.inspectorSections(.top).isEmpty)
        }
    }

    /// Menus come in `MenuID.order` (Plates, 5, between View and Model), whichever feature names them first; a
    /// menu's sections in their `order` across features, ties in the feature-list order.
    @Test func menuSectionsInterleaveByOrderAcrossFeaturesAndMenusByTheirOrder() {
        withProbes(.init(menu: [(.model, 20, ["a1"]), (.view, 0, ["a0"])]),
                   .init(menu: [(.model, 10, ["b1", "b2"])]),
                   .init(menu: [(MenuID(title: "Plates", order: 5), 0, ["c"]), (.model, 10, ["c1"])])) { _, _ in
            #expect(FeatureSet.menuSnapshot(types: probeTypes, in: nil)
                    == ["View", "  a0", "Plates", "  c", "Model", "  b1", "  b2", "  c1", "  a1"])
        }
    }

    @Test func eachSetBuildsItsOwnInstancesAndFindsThemByType() {
        let one = ViewerModel(), two = ViewerModel()
        let a = FeatureSet(types: probeTypes, host: one), b = FeatureSet(types: probeTypes, host: two)
        #expect(a[PA.self] != nil)
        #expect(a[PA.self] !== b[PA.self])
        #expect(a[PA.self] !== FeatureSet(types: probeTypes, host: one)[PA.self])     // a second set on the same window too
        #expect(a[RotateFeature.self] == nil)
        // the real features: one instance per window
        #expect(one.features[RotateFeature.self] != nil && one.features[RotateFeature.self] !== two.features[RotateFeature.self])
        withExtendedLifetime((one, two)) {}
    }

    /// `FeatureCommands` shows a fixed number of menus.
    @Test func theMenuBarHasRoomForEveryMenu() {
        #expect(FeatureSet.menuIDs().filter { !$0.isFileGroup }.count <= FeatureCommands.menuSlots)
    }

    // MARK: the real features

    @Test func theWindowsOverlaysStackAsTheyAlwaysHave() throws {
        let model = ViewerModel()
        var expected = ["empty-state", "status", "build-spinner", "lay-face-hint", "view-cube-and-toolbar", "plate-toast"]
        #if DEBUG
        expected.insert("frame-time", at: 0)
        #endif
        #expect(model.features.overlays.map(\.id) == expected)
        #expect(model.toolbar.slots.map(\.id) == ["rotate", "suggest", "strength", "view"])
        #expect(model.features.windowToolbarItems.map(\.id) == ["export"])
        #expect(model.features.inspectorSections(.top).map(\.id) == ["readout", "plate-part-buttons"])
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

    /// The menu bar, as it has always been: View, Model, then File's Export and Add Model, and Edit's Remove Model. Keep in step with docs/UI-MAP.md.
    @Test func theMenuBarKeepsItsItemsAndTheirOrder() {
        var view = ["View", "  Show Rotate Rings", "  Rebuild While Dragging", "  Frame Part [0]", "  Home", "  Show Print Layers"]
        #if DEBUG
        view.append("  Show Frame Time")
        #endif
        view += ["  -", "  Front [1]", "  Back [2]", "  Left [3]", "  Right [4]", "  Top [5]", "  Bottom [6]"]
        let model = ["Model", "  Rotate 90° About X", "  Rotate 90° About Y", "  Rotate 90° About Z", "  Reset Rotation", "  -",
                     "  Lay a Face Flat", "  -", "  Suggest Orientation", "  Strength Arrow…"]
        let file = ["File", "  Export", "  > STL", "  > 3MF", "  > Fins only (STL)", "  Export Again [e]", "  Add Model to Plate…"]
        let edit = ["Edit", "  Remove Model from Plate"]
        // no window focused
        #expect(FeatureSet.menuSnapshot(in: nil) == view + model + file + edit)
        // a window with nothing to export: the same items (the note under Export appears once there is a build)
        let window = ViewerModel()
        #expect(FeatureSet.menuSnapshot(in: window.features) == view + model + file + edit)
    }

    /// The menu bar can hold the features of a window that has just closed: the menus read them as no window.
    @Test func menusReadAClosedWindowsFeaturesWithoutCrashing() {
        var window: ViewerModel? = ViewerModel()
        let features = window!.features!
        #expect(features[RotateFeature.self] != nil)
        window = nil
        #expect(features[RotateFeature.self] == nil)
        #expect(FeatureSet.menuSnapshot(in: features) == FeatureSet.menuSnapshot(in: nil))
    }
}
