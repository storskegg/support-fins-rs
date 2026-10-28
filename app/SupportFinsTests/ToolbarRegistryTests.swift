import SwiftUI
import Testing
@testable import Support_Fins

@MainActor
struct ToolbarRegistryTests {
    @Test func aSlotIsEmptyUntilATrackRegistersIt() {
        let r = ToolbarRegistry()
        #expect(ToolbarSlot.allCases.allSatisfy { r.badges[$0] == nil })
        var lit = false
        r.register(.strength, isLit: { lit }) { Text("pad") }
        #expect(r.badges[.strength] != nil && r.badges[.suggest] == nil)
        #expect(r.badges[.strength]?.isLit() == false)
        lit = true
        #expect(r.badges[.strength]?.isLit() == true)
    }

    @Test func oneIsOpenAtATimeAndFoldingClosesIt() {
        let r = ToolbarRegistry()
        r.toggle(.rotate)
        #expect(r.open == .rotate)
        r.toggle(.suggest)
        #expect(r.open == .suggest)
        r.toggle(.suggest)
        #expect(r.open == nil)
        r.toggle(.rotate); r.fold()
        #expect(r.open == nil)
    }

    @Test func unregisteringTheOpenSlotFoldsIt() {
        let r = ToolbarRegistry()
        r.register(.suggest) { Text("x") }
        r.toggle(.suggest)
        r.unregister(.suggest)
        #expect(r.open == nil && r.badges[.suggest] == nil)
    }

    @Test func aFlashShowsOverASlotThenClears() async throws {
        let r = ToolbarRegistry()
        r.flash("Z +90°", over: .rotate, seconds: 0.05)
        #expect(r.flash?.slot == .rotate && r.flash?.text == "Z +90°")
        try await Task.sleep(for: .milliseconds(200))
        #expect(r.flash == nil)
    }
}
