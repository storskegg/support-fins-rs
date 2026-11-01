import AppKit
import SwiftUI
import Testing

@testable import Support_Fins

/// The window toolbar's layout (user suggestion S5), read from a real `NSToolbar`: `ContentView` hosted in an
/// off-screen window with SwiftUI's toolbar bridging, at the default width and narrower, with the inspector open and
/// closed. No mouse, no Accessibility permission; the window is never on a screen.
///
/// - no two items overlap, and each control has its own capsule (Add fins was drawn inside the Overhang slider's);
/// - with the inspector open, the window's items stay left of it and the inspector toggle sits over it;
/// - in a narrow window Add fins, Export and the inspector toggle stay, the build volume and the slider go to »;
/// - the Overhang slider draws no tick marks (its 41 per-degree ticks were the faint second track under it).
@MainActor
@Suite("Window toolbar layout", .serialized)
struct WindowToolbarLayoutTests {
    private final class OffscreenWindow: NSWindow {
        override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect { frameRect }
    }

    struct Item {
        let label: String
        let visible: Bool
        let frame: CGRect
    }

    struct Layout {
        let items: [Item]
        /// The glass capsules the toolbar draws, in window coordinates.
        let capsules: [CGRect]
        /// The inspector column's left edge, when it is open.
        let inspectorMinX: CGFloat?
        let sliders: [NSSlider]

        func item(_ label: String) throws -> Item { try #require(items.first { $0.label == label }, "no item \(label)") }
    }

    private func views(under v: NSView) -> [NSView] { [v] + v.subviews.flatMap { views(under: $0) } }

    /// The toolbar of a `width`-point window, once it has laid out.
    private func layout(width: CGFloat, inspector: Bool) async throws -> Layout {
        let frame = NSRect(x: -10_000, y: -10_000, width: width, height: 760)
        let window = OffscreenWindow(contentRect: frame, styleMask: [.titled, .closable, .resizable, .miniaturizable, .fullSizeContentView],
                                     backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.toolbarStyle = .unified
        let host = NSHostingView(rootView: ContentView(document: PartDocument(filename: "layout.stl"), inspectorStarts: inspector))
        host.sceneBridgingOptions = [.toolbars, .title]
        window.contentView = host
        window.setFrame(frame, display: true)
        window.orderFront(nil)
        defer { window.orderOut(nil); window.contentView = nil }
        // wait for the items and the inspector to settle (the part loads, the inspector slides in or out)
        var last = ""
        var still = 0
        for _ in 0..<100 where still < 5 {
            try await Task.sleep(for: .milliseconds(50))
            let now = (window.toolbar?.items ?? []).map { "\($0.label)\($0.isVisible)\($0.view.map { $0.convert($0.bounds, to: nil) } ?? .zero)" }.joined()
            still = now == last && !now.isEmpty ? still + 1 : 0
            last = now
        }
        let toolbar = try #require(window.toolbar)
        let items = toolbar.items.filter { !$0.label.isEmpty }.map {
            Item(label: $0.label, visible: $0.isVisible, frame: $0.view.map { $0.convert($0.bounds, to: nil) } ?? .zero)
        }
        let all = views(under: try #require(host.superview))
        let capsules = all.filter { String(describing: type(of: $0)) == "NSToolbarPlatterView" && !$0.isHidden && $0.window != nil }
            .map { $0.convert($0.bounds, to: nil) }
        let split = all.compactMap { $0 as? NSSplitView }.first
        let inspectorMinX = split.flatMap { s in s.arrangedSubviews.count > 1 && !s.isSubviewCollapsed(s.arrangedSubviews[1]) && !s.arrangedSubviews[1].isHidden
            ? s.arrangedSubviews[1].convert(s.arrangedSubviews[1].bounds, to: nil).minX : nil }
        let sliders = toolbar.items.compactMap(\.view).flatMap { views(under: $0) }.compactMap { $0 as? NSSlider }
        return Layout(items: items, capsules: capsules, inspectorMinX: inspectorMinX, sliders: sliders)
    }

    nonisolated private static let sizes: [(CGFloat, Bool)] = [(1100, true), (1100, false), (800, true), (800, false), (700, true), (700, false)]

    @Test(arguments: sizes)
    func nothingOverlaps(width: CGFloat, inspector: Bool) async throws {
        let l = try await layout(width: width, inspector: inspector)
        let shown = l.items.filter(\.visible)
        for (i, a) in shown.enumerated() {
            for b in shown[(i + 1)...] {
                #expect(!a.frame.intersects(b.frame), "\(a.label) \(a.frame) overlaps \(b.label) \(b.frame)")
            }
        }
        for (i, a) in l.capsules.enumerated() {
            for b in l.capsules[(i + 1)...] { #expect(!a.intersects(b), "capsules \(a) and \(b) overlap") }
        }
        // one capsule per shown item: none shares one (Add fins sat in the Overhang slider's)
        for item in shown {
            let holders = l.capsules.filter { $0.insetBy(dx: -1, dy: -1).contains(item.frame) }
            #expect(holders.count == 1, "\(item.label) is in \(holders.count) capsules")
            if let c = holders.first {
                #expect(shown.filter { c.insetBy(dx: -1, dy: -1).contains($0.frame) }.count == 1, "\(item.label) shares its capsule")
            }
        }
    }

    @Test(arguments: [1100.0, 800.0, 700.0])
    func withTheInspectorOpenTheWindowsItemsStayLeftOfIt(width: CGFloat) async throws {
        let l = try await layout(width: width, inspector: true)
        let edge = try #require(l.inspectorMinX)
        for item in l.items where item.visible && item.label != "Inspector" {
            #expect(item.frame.maxX <= edge, "\(item.label) ends at \(item.frame.maxX), the inspector starts at \(edge)")
        }
        let toggle = try l.item("Inspector")
        #expect(toggle.visible && toggle.frame.minX >= edge, "the inspector toggle sits over the inspector")
    }

    @Test(arguments: sizes)
    func addFinsExportAndTheInspectorToggleStay(width: CGFloat, inspector: Bool) async throws {
        let l = try await layout(width: width, inspector: inspector)
        for label in ["Add fins", "Export", "Inspector"] {
            let item = try l.item(label)
            #expect(item.visible, "\(label) at \(width) pt, inspector \(inspector)")
        }
    }

    @Test func atTheDefaultSizeEveryItemShows() async throws {
        for inspector in [true, false] {
            let l = try await layout(width: 1100, inspector: inspector)
            let hidden = l.items.filter { !$0.visible }.map(\.label)
            #expect(hidden.isEmpty, "\(hidden) hidden, inspector \(inspector)")
        }
    }

    @Test func theOverhangSliderHasNoTickMarks() async throws {
        let l = try await layout(width: 1100, inspector: false)
        let slider = try #require(l.sliders.first)
        #expect(slider.numberOfTickMarks == 0)
    }
}
