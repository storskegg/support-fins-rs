import Foundation
import SwiftUI
import Testing
import Viewer
@testable import Support_Fins

@MainActor
@Suite("View popover")
struct ViewSettingsTests {
    func defaults() -> UserDefaults {
        let d = UserDefaults(suiteName: "ViewSettingsTests-\(UUID().uuidString)")!
        return d
    }

    @Test func theMousePresetStartsAsDefault() {
        let s = ViewSettings(controller: nil, defaults: defaults())
        #expect(s.navPreset == .default)
    }

    @Test func theMousePresetIsRememberedUnderTheWebsKey() {
        let d = defaults()
        let s = ViewSettings(controller: nil, defaults: d)
        s.navPreset = .blender
        #expect(d.string(forKey: "sf.navPreset") == "blender")
        #expect(ViewSettings(controller: nil, defaults: d).navPreset == .blender)
    }

    @Test func anUnknownStoredKeyFallsBackToDefault() {
        let d = defaults()
        d.set("toString", forKey: "sf.navPreset")
        #expect(ViewSettings(controller: nil, defaults: d).navPreset == .default)
    }

    @Test func thePresetReachesTheViewer() throws {
        let c = try #require(ViewerController(), "no Metal device")
        let d = defaults()
        d.set("onshape", forKey: "sf.navPreset")
        let s = ViewSettings(controller: c, defaults: d)
        #expect(c.navPreset == .onshape)
        s.navPreset = .fusion
        #expect(c.navPreset == .fusion)
    }

    @Test func theViewSlotIsLastWithTheWebsTooltip() {
        #expect(ToolbarSlot.allCases.last == .view)
        let r = ToolbarRegistry()
        r.register(.view) { Text("x") }
        #expect(r.badges[.view] != nil)
    }
}
