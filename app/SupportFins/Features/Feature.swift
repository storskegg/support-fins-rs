import SupportFinsCore
import SwiftUI

/// One feature of the window, living in its own folder. A window builds one instance of each feature in
/// `Features.all` (`FeatureSet`); the instance owns the feature's models and contributes through the properties
/// below, all optional. `ContentView` and `FeatureCommands` compose the contributions in a fixed order, so a
/// feature never edits them. See Features/README.md.
@MainActor
protocol Feature: AnyObject {
    /// `host` is the window's shared models (session, controller, build wiring, toolbar). A feature keeps its own
    /// state in itself, not in `ViewerModel`.
    init(host: ViewerModel)

    /// Items for the menu bar. The menu bar exists with no window focused, so this takes the instance of the
    /// focused window, or nil: items then show with nothing to act on.
    static func menus(_ instance: Self?) -> [MenuSection]

    var overlays: [WindowOverlay] { get }
    var modifiers: [WindowModifier] { get }
    var windowToolbarItems: [WindowToolbarItem] { get }
    /// Slots of the floating toolbar this feature adds; the bar shows them in `order`, dimmed until `registerTools`
    /// gives them a badge.
    var toolbarSlots: [ToolbarSlot] { get }
    var inspectorSections: [InspectorSection] { get }
    var statusLines: [StatusLine] { get }
    var escapeHandlers: [EscapeHandler] { get }

    /// Register the badges of this feature's slots. Runs once with the window; a feature whose popover depends on
    /// the part calls it again when the part changes.
    func registerTools(in toolbar: ToolbarRegistry)
    /// A part is about to replace the one on show, and after `session` and the viewer have taken it.
    func partWillShow()
    func partDidShow(_ parts: [LoadedPart])
}

extension Feature {
    static func menus(_ instance: Self?) -> [MenuSection] { [] }
    var overlays: [WindowOverlay] { [] }
    var modifiers: [WindowModifier] { [] }
    var windowToolbarItems: [WindowToolbarItem] { [] }
    var toolbarSlots: [ToolbarSlot] { [] }
    var inspectorSections: [InspectorSection] { [] }
    var statusLines: [StatusLine] { [] }
    var escapeHandlers: [EscapeHandler] { [] }
    func registerTools(in toolbar: ToolbarRegistry) {}
    func partWillShow() {}
    func partDidShow(_ parts: [LoadedPart]) {}
}

extension [LoadedPart] {
    /// The engine's part when exactly one object is on show (the tools that need a single part).
    var singleCorePart: Part? { count == 1 ? (self[0].mesh as? CorePart)?.part : nil }
}
