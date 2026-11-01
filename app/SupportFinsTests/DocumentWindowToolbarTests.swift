import AppKit
import Foundation
import SwiftUI
import Testing

@testable import Support_Fins

/// The toolbar's priorities in a real document window (the app's `DocumentGroup`), not the hosting view
/// `WindowToolbarLayoutTests` builds. The UI run of 2026-11-01 opened the German window with Add fins and Export in the
/// toolbar's » and the build volume and the Overhang slider still showing: every item kept the standard priority, so
/// AppKit hid the trailing ones first. SwiftUI configures a document window's items after their views (and the
/// priority markers in them) are in the window, and sets each item's priority back to standard as it does; the hosting
/// view of `WindowToolbarLayoutTests` does not, so that test passed. Here the document is opened as the app opens it,
/// its window kept invisible (alpha 0, behind every window, never key): no mouse, nothing on screen.
@MainActor
@Suite("Document window toolbar", .serialized)
struct DocumentWindowToolbarTests {
    /// A 10 mm cube as a binary STL in `dir`.
    private func writeCube(in dir: URL) throws -> URL {
        let v: [[Float]] = [[0, 0, 0], [10, 0, 0], [10, 10, 0], [0, 10, 0], [0, 0, 10], [10, 0, 10], [10, 10, 10], [0, 10, 10]]
        let faces = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]]
        var data = Data(count: 80)
        var count = UInt32(faces.count)
        withUnsafeBytes(of: &count) { data.append(contentsOf: $0) }
        for f in faces {
            for x in [Float](repeating: 0, count: 3) + f.flatMap({ v[$0] }) { var x = x; withUnsafeBytes(of: &x) { data.append(contentsOf: $0) } }
            data.append(contentsOf: [0, 0])
        }
        let url = dir.appending(path: "cube.stl")
        try data.write(to: url)
        return url
    }

    /// (label, shown) of the window's labelled toolbar items, the Overhang slider (no label) as "Overhang".
    private func items(_ w: NSWindow) -> [(label: String, visible: Bool, priority: Int)] {
        (w.toolbar?.items ?? []).filter { !$0.label.isEmpty || $0.view != nil }
            .map { ($0.label.isEmpty ? "Overhang" : $0.label, $0.isVisible, $0.visibilityPriority.rawValue) }
    }

    /// The cube opened as the app opens a file, in an invisible window; `body` gets the window. The document closes after.
    private func withDocumentWindow(_ body: (NSWindow) async throws -> Void) async throws {
        let dir = FileManager.default.temporaryDirectory.appending(path: "sf-app-tests/doc-toolbar-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = try writeCube(in: dir)
        let controller = NSDocumentController.shared
        let doc = try controller.makeDocument(withContentsOf: url, ofType: try controller.typeForContents(of: url))
        controller.addDocument(doc)
        defer { doc.close() }
        doc.makeWindowControllers()
        let window = try #require(doc.windowControllers.first?.window)
        window.alphaValue = 0
        window.orderBack(nil)
        try await body(window)
    }

    private func get(_ o: NSObject, _ sel: String) -> Any? {
        let s = NSSelectorFromString(sel)
        return o.responds(to: s) ? o.perform(s)?.takeUnretainedValue() : nil
    }

    /// The accessibility element with identifier `id` under `root`, as XCUITest finds it.
    private func axElement(_ id: String, under root: Any) -> NSObject? {
        var found: NSObject?
        func walk(_ e: Any, _ depth: Int) {
            guard found == nil, depth < 40, let o = e as? NSObject else { return }
            if get(o, "accessibilityIdentifier") as? String == id { found = o; return }
            for c in (get(o, "accessibilityChildren") as? [Any]) ?? [] { walk(c, depth + 1) }
        }
        walk(root, 0)
        return found
    }

    /// An element's (description, value) as another process reads it: through the AX API (`AXUIElement`) on this app's
    /// own pid, from a background thread while the main thread serves the request, which is the path XCUITest and
    /// VoiceOver take (no Accessibility permission needed for one's own process). The in-process `accessibilityLabel`
    /// already read "Fins on" while this path still said "Add fins": the UI run of 2026-11-01 (#263) failed on it with
    /// 'the toggle reads "Add fins", its value Optional(1)'.
    private func remote(_ id: String) async -> (label: String?, value: Int?) {
        let pid = getpid()
        return await Task.detached {
            let app = AXUIElementCreateApplication(pid)
            AXUIElementSetMessagingTimeout(app, 3)
            func attr(_ e: AXUIElement, _ a: String) -> AnyObject? {
                var v: AnyObject?
                return AXUIElementCopyAttributeValue(e, a as CFString, &v) == .success ? v : nil
            }
            var hit: AXUIElement?
            func walk(_ e: AXUIElement, _ depth: Int) {
                guard hit == nil, depth < 40 else { return }
                if attr(e, "AXIdentifier") as? String == id { hit = e; return }
                for c in (attr(e, "AXChildren") as? [AXUIElement]) ?? [] { walk(c, depth + 1) }
            }
            walk(app, 0)
            guard let hit else { return (nil, nil) }
            return (attr(hit, "AXDescription") as? String, (attr(hit, "AXValue") as? NSNumber)?.intValue)
        }.value
    }

    /// Add fins turned on from the Fins row (and off again from the toggle): the toolbar toggle's words follow its state
    /// as another process reads them. SwiftUI's toolbar item kept the label it was made with for remote clients;
    /// `FinsToggle` gives the toggle a new identity per state.
    @Test func theToggleSaysFinsOnToOtherProcessesWhenTheFinsRowAddsThem() async throws {
        _ = NSApp.perform(NSSelectorFromString("accessibilitySetValue:forAttribute:"), with: true as NSNumber,
                          with: "AXEnhancedUserInterface" as NSString)
        try await withDocumentWindow { window in
            var add: NSObject?
            for _ in 0..<50 where add == nil {
                try await Task.sleep(for: .milliseconds(100))
                add = axElement("stat-add-fins", under: window)
            }
            let off = await remote("fins-toggle")
            #expect(off.label == "Add fins" && off.value == 0, "off: the toggle reads \(String(describing: off.label)), its value \(String(describing: off.value))")
            _ = try #require(add, "the Fins row offers Add fins").perform(NSSelectorFromString("accessibilityPerformPress"))
            var on = (label: String?.none, value: Int?.none)
            for _ in 0..<30 {
                try await Task.sleep(for: .milliseconds(100))
                on = await remote("fins-toggle")
                if on.value == 1 && on.label == "Fins on" { break }
            }
            #expect(on.label == "Fins on" && on.value == 1, "on: the toggle reads \(String(describing: on.label)), its value \(String(describing: on.value))")
            // off again, from the toggle itself
            _ = try #require(axElement("fins-toggle", under: window)).perform(NSSelectorFromString("accessibilityPerformPress"))
            var again = (label: String?.none, value: Int?.none)
            for _ in 0..<30 {
                try await Task.sleep(for: .milliseconds(100))
                again = await remote("fins-toggle")
                if again.value == 0 && again.label == "Add fins" { break }
            }
            #expect(again.label == "Add fins" && again.value == 0, "off again: the toggle reads \(String(describing: again.label)), its value \(String(describing: again.value))")
        }
    }

    @Test func aNarrowDocumentWindowKeepsAddFinsAndExport() async throws {
        try await withDocumentWindow { window in
        // the part loads and the toolbar settles at the default size, then the window narrows to 800 pt: too narrow for
        // every item (the German window at its default size was too)
        try await Task.sleep(for: .seconds(1.5))
        window.setFrame(NSRect(x: window.frame.minX, y: window.frame.minY, width: 800, height: window.frame.height), display: true)
        var shown: [(label: String, visible: Bool, priority: Int)] = []
        for _ in 0..<40 {
            try await Task.sleep(for: .milliseconds(100))
            shown = items(window)
            if shown.contains(where: { !$0.visible }) { break }
        }
        try await Task.sleep(for: .milliseconds(300))
        shown = items(window)
        let state = shown.map { "\($0.label) \($0.visible ? "shown" : "in »") (priority \($0.priority))" }.joined(separator: ", ")
        #expect(shown.contains { !$0.visible }, "800 pt is narrow enough to send some item to »: \(state)")
        for label in ["Add fins", "Export", "Inspector"] {
            #expect(shown.first { $0.label == label }?.visible == true, "\(label) stays in an 800 pt document window: \(state)")
        }
        // the low-priority items go first
        #expect(shown.first { $0.label == "Overhang" }?.visible == false, "the Overhang slider goes to » first: \(state)")
        }
    }
}
