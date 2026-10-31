import AppKit
import Observation
import SwiftUI
import Testing
@testable import Support_Fins

/// Typing into a `NumberField` through its field editor, as a person does, one key at a time. The UI test
/// `testPickingThePrinterSetsTheBedAndTheLayerHeight` typed 0.1 over the printer's 0.05 and the field read "0.01.1": the
/// first key "0" was read as 0, held to the 0.01 minimum, and the held value was written back over the text being typed.
/// The web holds its value to the limits on every `input` too (`wireGap`, `web/ui/settings.js`) but leaves the text alone.
@MainActor
@Suite("Number field typing", .serialized)
struct NumberFieldTests {
    @Observable final class Box {
        var value: Double
        /// Anything else the window shows: the app redraws the inspector after a setting changes (the fins rebuild, the
        /// status card, the undo step), and that later redraw is what wrote the held value over the typed text.
        var tick = 0
        init(_ value: Double) { self.value = value }
    }

    private struct Host: View {
        let box: Box
        let limits: FinSettings.Limits
        var body: some View {
            VStack {
                NumberField(title: "Layer height", value: Binding(get: { box.value }, set: { box.value = $0 }), limits: limits,
                            unit: "mm", fieldID: "layer-height")
                TextField("other", text: .constant("x"))
                Text(verbatim: "\(box.tick)")
            }
            .environment(\.locale, Locale(identifier: "en_US"))
            .padding().frame(width: 320, height: 100)
        }
    }

    private func fields(in view: NSView) -> [NSTextField] {
        var out: [NSTextField] = []
        if let f = view as? NSTextField, f.isEditable { out.append(f) }
        for v in view.subviews { out += fields(in: v) }
        return out
    }

    /// Let SwiftUI catch up with the field editor's changes and with the box's.
    private func settle(_ hosting: NSView) {
        for _ in 0..<5 {
            RunLoop.main.run(until: Date().addingTimeInterval(0.02))
            hosting.layoutSubtreeIfNeeded()
        }
    }

    private func host(_ box: Box, limits: FinSettings.Limits = FinSettings.layerHeightLimits)
        -> (window: NSWindow, hosting: NSView, field: NSTextField, other: NSTextField)? {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 100), styleMask: [.titled],
                              backing: .buffered, defer: false)
        let hosting = NSHostingView(rootView: Host(box: box, limits: limits))
        window.contentView = hosting
        window.layoutIfNeeded()
        settle(hosting)
        let all = fields(in: hosting)
        guard all.count == 2 else { return nil }
        return (window, hosting, all[0], all[1])
    }

    /// Click into the field (first responder), ⌘A, then each character typed on its own; the editor's text after each.
    private func type(_ s: String, into field: NSTextField, of window: NSWindow, hosting: NSView, box: Box) throws -> [String] {
        #expect(window.makeFirstResponder(field))
        let editor = try #require(field.currentEditor() as? NSTextView, "the field is being edited")
        editor.selectAll(nil)
        var seen: [String] = []
        for c in s {
            editor.insertText(String(c), replacementRange: editor.selectedRange())
            settle(hosting)
            box.tick += 1
            settle(hosting)
            seen.append((field.currentEditor() as? NSTextView)?.string ?? "<no editor>")
        }
        return seen
    }

    @Test("0.1 typed over 0.05 shows as typed, key by key, and the value follows")
    func typedOverAHigherMinimum() throws {
        let box = Box(0.05)
        let (window, hosting, field, _) = try #require(host(box))
        #expect(field.stringValue == "0.05")
        let seen = try type("0.1", into: field, of: window, hosting: hosting, box: box)
        #expect(seen == ["0", "0.", "0.1"], "the field after each key: \(seen)")
        #expect(box.value == 0.1)
    }

    @Test("0.15 typed into a Support gap (0.1 minimum) over 0.2")
    func typedIntoTheGap() throws {
        let box = Box(0.2)
        let (window, hosting, field, _) = try #require(host(box, limits: FinSettings.gapLimits))
        let seen = try type("0.15", into: field, of: window, hosting: hosting, box: box)
        #expect(seen == ["0", "0.", "0.1", "0.15"], "the field after each key: \(seen)")
        #expect(box.value == 0.15)
    }

    @Test("the value is held to the limits while typing; the text shows it once the edit ends")
    func heldWhileTypingShownAfter() throws {
        let box = Box(0.05)
        let (window, hosting, field, other) = try #require(host(box))
        let seen = try type("0.9", into: field, of: window, hosting: hosting, box: box)
        #expect(seen == ["0", "0.", "0.9"], "\(seen)")
        #expect(box.value == 0.6, "held to the 0.6 maximum while typing, as the web's input event does")
        #expect(window.makeFirstResponder(other), "the edit ends: focus moves on")
        settle(hosting)
        #expect(field.stringValue == "0.6", "the field shows the held value: \(field.stringValue)")
    }

    @Test("a value set from outside (a printer picked, the stepper) shows in the field")
    func outsideChangeShows() throws {
        let box = Box(0.2)
        let (_, hosting, field, _) = try #require(host(box))
        box.value = 0.05
        settle(hosting)
        #expect(field.stringValue == "0.05")
    }
}
