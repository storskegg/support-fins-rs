import AppKit
import Foundation
import SupportFinsCore
import SwiftUI
import Testing

@testable import Support_Fins

/// S2 (November 2026): "there is no way to add the fins". Ten coins merged into one part, Full coverage, Plate ties on:
/// the toolbar's Add fins showed as an unlabelled icon, the Fins row read "—", a failed build said nothing, and the
/// ties locked Arrange and Move on a plate with nothing to tie. Each is checked here without the mouse.
@MainActor
@Suite("Add fins is found")
struct AddFinsFoundTests {
    /// (identifier, label) of every accessibility element under `root` (as `ToolbarAccessibilityTests` reads them).
    private func elements(under root: Any) -> [(id: String?, label: String?)] {
        func get(_ o: NSObject, _ sel: String) -> Any? {
            let s = NSSelectorFromString(sel)
            return o.responds(to: s) ? o.perform(s)?.takeUnretainedValue() : nil
        }
        var out: [(id: String?, label: String?)] = []
        func walk(_ e: Any, _ depth: Int) {
            guard depth < 40, let o = e as? NSObject else { return }
            out.append((get(o, "accessibilityIdentifier") as? String, get(o, "accessibilityLabel") as? String))
            for c in (get(o, "accessibilityChildren") as? [Any]) ?? [] { walk(c, depth + 1) }
        }
        walk(root, 0)
        return out
    }

    private func host<V: View>(_ view: V, width: CGFloat = 600, height: CGFloat = 200) async throws -> NSHostingView<V> {
        _ = NSApp.perform(NSSelectorFromString("accessibilitySetValue:forAttribute:"), with: true as NSNumber,
                          with: "AXEnhancedUserInterface" as NSString)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: width, height: height), styleMask: [.titled],
                              backing: .buffered, defer: false)
        let host = NSHostingView(rootView: view)
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        try await Task.sleep(for: .milliseconds(200))
        return host
    }

    // MARK: the toolbar's Add fins

    /// A window toolbar draws its items icon-only; the Add fins toggle keeps its words. Measured as the toolbar lays it
    /// out: with an icon-only label style around it, it is still as wide as its title.
    @Test func addFinsShowsItsTitleWhereTheToolbarWouldShowOnlyTheIcon() async throws {
        let session = PartSession()
        let toggle = try await host(FinsToggle(session: session).labelStyle(.iconOnly).fixedSize())
        let iconOnly = try await host(Toggle("Add fins", systemImage: "square.stack.3d.up", isOn: .constant(false))
            .toggleStyle(.button).labelStyle(.iconOnly).fixedSize())
        let width = toggle.fittingSize.width, bare = iconOnly.fittingSize.width
        #expect(width > bare + 30, "the title is drawn beside the icon: \(width) pt against \(bare) pt icon-only")
    }

    // MARK: Placement shows the mode in force

    /// The chosen mode stands out from the others in a window that is not the key one too (as a test's never is): with
    /// `.bordered` and an accent tint, Auto chosen drew exactly as Full coverage chosen above the note, and the user read
    /// the chosen Auto as switched off.
    @Test func placementDrawsTheChosenModeApart() async throws {
        func render(_ mode: FinSettings.Mode) async throws -> NSBitmapImageRep {
            var s = FinSettings()
            s.mode = mode
            let view = Form { Section { PlacementSection(settings: .constant(s)) } }.formStyle(.grouped).frame(width: 320, height: 160)
            let h = try await host(view, width: 320, height: 160)
            let rep = try #require(h.bitmapImageRepForCachingDisplay(in: h.bounds))
            h.cacheDisplay(in: h.bounds, to: rep)
            return rep
        }
        let auto = try await render(.auto), full = try await render(.full)
        try #require(auto.pixelsWide == full.pixelsWide && auto.pixelsHigh == full.pixelsHigh)
        // the buttons' row, above the mode's note (the note's words differ by mode anyway)
        var differing = 0
        for y in 0..<(auto.pixelsHigh / 2) {
            for x in 0..<auto.pixelsWide {
                guard let a = auto.colorAt(x: x, y: y)?.usingColorSpace(.deviceRGB), let b = full.colorAt(x: x, y: y)?.usingColorSpace(.deviceRGB) else { continue }
                if abs(a.brightnessComponent - b.brightnessComponent) > 0.1 { differing += 1 }
            }
        }
        #expect(differing > 500, "the buttons look the same whichever mode is chosen (\(differing) pixels differ)")
    }

    // MARK: the Fins row

    @Test func theFinsRowOffersAddFinsWhileTheyAreOffAndItTurnsThemOn() async throws {
        let model = ViewerModel()
        let controller = try #require(model.controller, "no Metal device")
        let stl = URL(fileURLWithPath: try TestParts.write(TestParts.box(-5, 5, -5, 5, 0, 10), name: "s2-row-\(UUID().uuidString).stl"))
        defer { Scratch.remove(stl) }
        await model.load(url: stl)
        try #require(model.hasPart)
        func rowButton() async throws -> Bool {
            let view = Form { ReadoutSections(controller: controller, session: model.session, build: model.wiring?.build) }.formStyle(.grouped)
            return elements(under: try await host(view, width: 360, height: 700)).contains { $0.id == "stat-add-fins" }
        }
        #expect(!model.session.finsOn)
        #expect(try await rowButton(), "the Fins row offers Add fins while they are off")
        model.session.perform(JS.t("Add fins")) { model.session.finsOn = true }
        #expect(try await !rowButton(), "and not once they are on")
        model.session.perform(JS.t("Remove fins")) { model.session.finsOn = false }
    }

    // MARK: a failed build

    @Test func aFailedBuildSaysSoOnTheStatusCard() async throws {
        #expect(ReadoutFeature.buildFailure(finsOn: true, building: false, error: nil).isEmpty)
        #expect(ReadoutFeature.buildFailure(finsOn: false, building: false, error: "x").isEmpty, "fins off: no build to fail")
        #expect(ReadoutFeature.buildFailure(finsOn: true, building: true, error: "x").isEmpty, "a rebuild is out")
        let line = try #require(ReadoutFeature.buildFailure(finsOn: true, building: false, error: "the part is too large").first)
        #expect(line.text == "The supports could not be built: the part is too large. Change a setting or the pose to try again.")

        // the window's own: a build the engine refuses (a wall-line count it does not take) lands as that line
        let model = ViewerModel()
        try #require(model.controller != nil, "no Metal device")
        let stl = URL(fileURLWithPath: try TestParts.write(TestParts.box(-5, 5, -5, 5, 0, 10), name: "s2-fail-\(UUID().uuidString).stl"))
        defer { Scratch.remove(stl) }
        await model.load(url: stl)
        let build = try #require(model.wiring?.build)
        model.session.perform("Add Fins") { model.session.finsOn = true }
        try await eventually("the first build") { build.latest != nil && !build.building }
        #expect(!model.features.statusLines.contains { $0.id == "build-error" })
        build.build(rotation: model.session.pose.columnMajor, optionsJSON: FinSettings().optionsJSON(), tunablesJSON: "{\"wallLines\":3}")
        try await eventually("the refusal") { !build.building && build.error != nil }
        let failed = try #require(model.features.statusLines.first { $0.id == "build-error" })
        #expect(failed.text.hasPrefix("The supports could not be built: ") && failed.text.contains("wallLines"), "\(failed.text)")
        model.session.perform("Remove Fins") { model.session.finsOn = false }
    }

    // MARK: plate ties on one model

    @Test func mergedObjectsAreReadFromTheImportNote() {
        #expect(PlateTiesModel.mergedObjects(notes: ["3MF: merged 10 of 10 objects into one part."]) == 10)
        #expect(PlateTiesModel.mergedObjects(notes: ["cut out 1 negative part", "merged 2 of 5 objects into one part"]) == 2)
        #expect(PlateTiesModel.mergedObjects(notes: ["3MF: imported “Body_13” of 10 objects."]) == nil)
        #expect(PlateTiesModel.mergedObjects(notes: []) == nil)
    }

    @Test func tiesOnOneModelLockNothingAndSayTheyNeedSeparateModels() async throws {
        let model = ViewerModel()
        try #require(model.controller != nil, "no Metal device")
        let coin = try PlateTiesTests.coin("s2-alone")
        defer { Scratch.remove(coin) }
        await model.load(url: coin)
        let board = try #require(model.features[PlatesFeature.self]?.board)
        let ties = try #require(model.features[PlateTiesFeature.self]?.model)
        let move = try #require(model.features[MoveFeature.self]?.model)
        move.setOn(true)
        try #require(move.isOn)
        ties.setOn(true)
        #expect(ties.state.on && ties.alone)
        #expect(move.isOn, "Move stays on: there is nothing to tie")
        #expect(board.lockReason() == nil)
        let lines = model.features.statusLines
        #expect(!lines.contains { $0.id == "plate-ties-lock" || $0.id == "plate-ties-none" }, "\(lines.map(\.id))")
        let alone = try #require(lines.first { $0.id == "plate-ties-alone" })
        #expect(alone.text == "Plate ties join separate models, and this plate has one. Use Add model to put the models to tie on the plate.")
        move.setOn(false)

        // a second model: the ties have something to hold, and the lock is back (Move goes off)
        let other = try PlateTiesTests.coin("s2-second")
        defer { Scratch.remove(other) }
        move.setOn(true)
        await model.load(url: other, adding: true)
        try #require(board.plate.models.count == 2)
        board.sync()
        #expect(!ties.alone && board.lockReason() != nil)
        #expect(!move.isOn, "a tied plate of two keeps its models where they stand")
        #expect(model.features.statusLines.contains { $0.id == "plate-ties-lock" })
        ties.setOn(false)
    }

    @Test func tiesOnAMergedPartSayItsObjectsWereMerged() async throws {
        let model = ViewerModel()
        try #require(model.controller != nil, "no Metal device")
        await model.load(url: try OpenFlowTests.threeObjects())
        let picker = try #require(model.picker)
        for e in picker.entries { picker.set(e.id, true) }
        picker.load()
        #expect(await eventually { model.hasPart })
        let ties = try #require(model.features[PlateTiesFeature.self]?.model)
        ties.setOn(true)
        #expect(ties.mergedObjects == 3)
        let alone = try #require(model.features.statusLines.first { $0.id == "plate-ties-alone" })
        #expect(alone.text.hasPrefix("Plate ties join separate models, and this plate has one: its 3 objects were merged into one part."), "\(alone.text)")
        #expect(model.features[PlatesFeature.self]?.board.lockReason() == nil)
        ties.setOn(false)
    }
}
