import AppKit
import SwiftUI
import Testing
@testable import Support_Fins

/// Keyboard focus raises a tip card (`TipFocusProbe`): the window's first responder moves into the setting's frame.
/// A number field is an NSTextField behind SwiftUI's TextField, whose first responder is the window's field editor.
@MainActor
@Suite("Tip card focus", .serialized)
struct TipFocusTests {
    private func field(in view: NSView) -> NSTextField? {
        if let f = view as? NSTextField, f.isEditable { return f }
        for v in view.subviews { if let f = field(in: v) { return f } }
        return nil
    }

    private func host(_ changes: @escaping (Bool) -> Void) -> (NSWindow, NSTextField)? {
        struct Row: View {
            let changes: (Bool) -> Void
            @State var text = "1"
            var body: some View {
                VStack {
                    TextField("a", text: $text).background(TipFocusProbe { changes($0) })
                    TextField("b", text: $text)
                }.padding().frame(width: 300, height: 120)
            }
        }
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 120), styleMask: [.titled], backing: .buffered, defer: false)
        let hosting = NSHostingView(rootView: Row(changes: changes))
        window.contentView = hosting
        window.layoutIfNeeded()
        hosting.layoutSubtreeIfNeeded()
        guard let f = field(in: hosting) else { return nil }
        return (window, f)
    }

    @Test("focus that arrives by a key shows, and leaving hides")
    func keyFocus() throws {
        var seen: [Bool] = []
        let (window, first) = try #require(host { seen.append($0) })
        TipFocusProbe.keyDriven = { true }
        defer { TipFocusProbe.keyDriven = { NSApp.currentEvent?.type == .keyDown } }
        #expect(window.makeFirstResponder(first))
        #expect(seen == [true], "focus entered the field: \(seen)")
        window.makeFirstResponder(nil)
        #expect(seen == [true, false], "focus left: \(seen)")
    }

    @Test("focus that arrives by a click does not show")
    func clickFocus() throws {
        var seen: [Bool] = []
        let (window, first) = try #require(host { seen.append($0) })
        TipFocusProbe.keyDriven = { false }
        defer { TipFocusProbe.keyDriven = { NSApp.currentEvent?.type == .keyDown } }
        #expect(window.makeFirstResponder(first))
        #expect(seen.isEmpty)
    }
}
