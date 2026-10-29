import AppKit
import SwiftUI

/// Draw (`web/ui/walls.js`, `web/ui/braceghost.js`): hand-placed breakaway walls and sway braces. The Draw
/// placement mode and "+ Add walls by hand" put the pointer to work on the part's surface; with Sway braces on, Auto
/// and Full coverage stand a brace on an upright side at a click. The inspector gets the augment toggle and the Draw
/// controls; the viewer the walls, ghosts and dots; Edit > Delete (and Backspace / Delete) removes the selected support.
@MainActor
final class DrawFeature: Feature {
    unowned let host: ViewerModel
    let model: DrawModel

    init(host: ViewerModel) {
        self.host = host
        model = DrawModel(host: host)
    }

    var overlays: [WindowOverlay] {
        // over the viewport, which fills the window under the toolbar: the label's coordinates are the view's
        [WindowOverlay("brace-tip", order: 80, alignment: .topLeading, ignoresSafeArea: true) { BraceTipView(motion: self.model.motion) }]
    }

    var modifiers: [WindowModifier] {
        [WindowModifier.onChange("draw", of: { self.model.syncKey }, initial: true) { _, _ in self.model.sync() }]
    }

    var inspectorSections: [InspectorSection] {
        [InspectorSection("augment", slot: .basics, order: -10) { AugmentToggle(model: self.model) },
         InspectorSection("draw-controls", slot: .basics, order: 10) { DrawControls(model: self.model) }]
    }

    var escapeHandlers: [EscapeHandler] {
        // after the armed pointer modes (lay a face flat, Remove fins), as `app.js:194-205` orders them
        [EscapeHandler(id: "draw", priority: EscapeHandler.pointerMode - 10) { self.model.escape() }]
    }

    func partWillShow() { model.partChanged() }
    func partDidShow(_ parts: [LoadedPart]) { model.sync() }

    /// Edit > Delete (⌫): take back the last point of the wall being drawn, or remove the selected support
    /// (`app.js:63-74`). Disabled when it would do nothing.
    static func menus(_ f: DrawFeature?) -> [MenuSection] {
        [MenuSection(menu: .edit, order: 20, entries: [
            .item("Delete", shortcut: .delete, help: "Take back the last point of the wall you're drawing, or remove the selected support.",
                  enabled: f?.model.canDelete ?? false) {
                // a text field has the keys: ⌫ is its own
                if let text = NSApp.keyWindow?.firstResponder as? NSTextView { text.deleteBackward(nil); return }
                f?.model.deleteMenu()
            },
        ])]
    }
}
