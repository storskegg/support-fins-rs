import AppKit
import SwiftUI
import Testing
@testable import Support_Fins

/// The view toolbar as accessibility (and so XCUITest) sees it, read in-process from the hosting view: no
/// Accessibility permission and no mouse. A badge is found by its identifier and says its text in its label; a
/// button has no value (`ScaleUITests` reads the label).
@MainActor
struct ToolbarAccessibilityTests {
    /// (identifier, label, value) of every accessibility element under `root`.
    private func elements(under root: Any) -> [(id: String?, label: String?, value: Any?)] {
        func get(_ o: NSObject, _ sel: String) -> Any? {
            let s = NSSelectorFromString(sel)
            return o.responds(to: s) ? o.perform(s)?.takeUnretainedValue() : nil
        }
        var out: [(id: String?, label: String?, value: Any?)] = []
        func walk(_ e: Any, _ depth: Int) {
            guard depth < 30, let o = e as? NSObject else { return }
            out.append((get(o, "accessibilityIdentifier") as? String, get(o, "accessibilityLabel") as? String,
                        get(o, "accessibilityValue")))
            for c in (get(o, "accessibilityChildren") as? [Any]) ?? [] { walk(c, depth + 1) }
        }
        walk(root, 0)
        return out
    }

    @Test func theScaleBadgeSaysItsPercentInItsLabel() async throws {
        // SwiftUI builds its accessibility tree only for a client that asks (VoiceOver, XCUITest): ask as one
        NSApp.accessibilitySetValue(true, forAttribute: NSAccessibility.Attribute(rawValue: "AXEnhancedUserInterface"))
        let r = ToolbarRegistry()
        r.declare([.rotate, .scale])
        var suffix = ""
        r.register(.scale, detail: { suffix }) { Text("pop") }
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 100), styleMask: [.titled],
                              backing: .buffered, defer: false)
        defer { window.orderOut(nil) }
        func badge() async throws -> (id: String?, label: String?, value: Any?)? {
            let host = NSHostingView(rootView: ViewToolbar(session: PartSession(), registry: r))
            window.contentView = host
            host.layoutSubtreeIfNeeded()
            try await Task.sleep(for: .milliseconds(200))
            return elements(under: host).first { $0.id == "toolbar-scale-badge" }
        }

        let plain = try #require(try await badge())
        #expect(plain.label == "Scale")
        suffix = " 150%"
        let scaled = try #require(try await badge())
        #expect(scaled.label == "Scale 150%")   // `#scale-now`: "Scale" + " 150%"
        #expect(scaled.value == nil)
    }
}
