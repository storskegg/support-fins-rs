import AppKit
import SwiftUI

/// Which window toolbar items stay when the window is too narrow for all of them (user suggestion S5). AppKit moves
/// items into the toolbar's » menu lowest `NSToolbarItem.visibilityPriority` first (trailing first among equals);
/// SwiftUI has no modifier for it, so an item's content carries a marker view and the window's items take the
/// priority of the marker inside them. Add fins and the inspector toggle stay longest, the build volume and the
/// Overhang slider go first.
extension View {
    func toolbarVisibilityPriority(_ priority: NSToolbarItem.VisibilityPriority) -> some View {
        background(ToolbarPriorityMarker(priority: priority).frame(width: 0, height: 0).accessibilityHidden(true))
    }
}

@MainActor
enum ToolbarPriorities {
    /// Give each of `window`'s toolbar items the priority of the marker in its view; an item without one keeps its own.
    static func apply(in window: NSWindow?) {
        guard let toolbar = window?.toolbar else { return }
        for item in toolbar.items {
            guard let view = item.view, let marker = marker(in: view) else { continue }
            if item.visibilityPriority != marker.priority { item.visibilityPriority = marker.priority }
        }
    }

    /// The marker under `view` (a toolbar item's content is a handful of views deep).
    private static func marker(in view: NSView) -> ToolbarPriorityMarkerView? {
        if let m = view as? ToolbarPriorityMarkerView { return m }
        for sub in view.subviews { if let m = marker(in: sub) { return m } }
        return nil
    }
}

/// The marker in a toolbar item's content. It holds its item at its priority: in a document window (the app's
/// `DocumentGroup`) SwiftUI configures the items after their views, and so the markers, are in the window, and sets each
/// item's priority back to standard as it does; with no marker moving again, every item stayed standard and AppKit sent
/// the trailing ones (Add fins, Export) to » first (the German window, UI run of 2026-11-01; `DocumentWindowToolbarTests`).
/// So the marker watches its item's priority and puts it back on the next turn of the main loop when anything else sets it.
final class ToolbarPriorityMarkerView: NSView {
    var priority: NSToolbarItem.VisibilityPriority = .standard {
        didSet { if priority != oldValue { holdPriority() } }
    }
    /// The item whose view holds this marker, once found, and the watch on its priority.
    private weak var item: NSToolbarItem?
    private var watch: NSKeyValueObservation?

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        ToolbarPriorities.apply(in: window)
        holdPriority()
    }

    /// Find the item (the window toolbar's item whose view holds this marker), watch its priority, and set it.
    func holdPriority() {
        guard let toolbar = window?.toolbar,
              let item = toolbar.items.first(where: { $0.view.map { self.isDescendant(of: $0) } ?? false }) else { return }
        if item !== self.item {
            self.item = item
            watch = item.observe(\.visibilityPriority) { [weak self] _, _ in
                // KVO reports on the thread that set it (the main one) while the setter runs: put it back after
                DispatchQueue.main.async { MainActor.assumeIsolated { self?.holdPriority() } }
            }
        }
        if item.visibilityPriority != priority { item.visibilityPriority = priority }
    }
}

private struct ToolbarPriorityMarker: NSViewRepresentable {
    var priority: NSToolbarItem.VisibilityPriority

    func makeNSView(context: Context) -> ToolbarPriorityMarkerView {
        let v = ToolbarPriorityMarkerView()
        v.priority = priority
        return v
    }

    func updateNSView(_ v: ToolbarPriorityMarkerView, context: Context) {
        v.priority = priority
        v.holdPriority()
    }
}
