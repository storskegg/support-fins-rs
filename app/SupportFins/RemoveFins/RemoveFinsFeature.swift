import SwiftUI

/// Remove fins (`ui/remove.js`): the toggle and Restore all in the inspector's basics, and Esc ending the mode.
@MainActor
final class RemoveFinsFeature: Feature {
    unowned let host: ViewerModel

    init(host: ViewerModel) { self.host = host }

    var inspectorSections: [InspectorSection] {
        [InspectorSection("remove-fins", slot: .basics) { RemoveFinsControls(model: self.host.wiring?.removal) }]
    }

    var escapeHandlers: [EscapeHandler] {
        [EscapeHandler(id: "remove-fins", priority: EscapeHandler.windowMode) { self.host.wiring?.escape() ?? false }]
    }
}
