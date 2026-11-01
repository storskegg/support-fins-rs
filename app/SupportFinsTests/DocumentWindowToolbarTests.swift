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

    @Test func theFinsRowsAddFinsTurnsTheToggleToFinsOn() async throws {
        _ = NSApp.perform(NSSelectorFromString("accessibilitySetValue:forAttribute:"), with: true as NSNumber,
                          with: "AXEnhancedUserInterface" as NSString)
        try await withDocumentWindow { window in
            var add: NSObject?
            for _ in 0..<50 where add == nil {
                try await Task.sleep(for: .milliseconds(100))
                add = axElement("stat-add-fins", under: window)
            }
            let toggle = try #require(axElement("fins-toggle", under: window))
            #expect(get(toggle, "accessibilityLabel") as? String == "Add fins")
            _ = try #require(add, "the Fins row offers Add fins").perform(NSSelectorFromString("accessibilityPerformPress"))
            var label = "", value = ""
            for _ in 0..<50 {
                try await Task.sleep(for: .milliseconds(100))
                let t = axElement("fins-toggle", under: window) ?? toggle
                label = get(t, "accessibilityLabel") as? String ?? "nil"
                value = "\(get(t, "accessibilityValue") ?? "nil")"
                if label == "Fins on" { break }
            }
            print("FINSPROBE label \(label) value \(value) items \(window.toolbar?.items.map(\.label) ?? [])")
            #expect(label == "Fins on", "the toggle's label \(label), its value \(value)")
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
