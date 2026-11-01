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

final class ToolbarPriorityMarkerView: NSView {
    var priority: NSToolbarItem.VisibilityPriority = .standard

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        ToolbarPriorities.apply(in: window)
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
        ToolbarPriorities.apply(in: v.window)
    }
}
