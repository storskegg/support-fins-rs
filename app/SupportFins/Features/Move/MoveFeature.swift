import AppKit
import SwiftUI
import Viewer

extension ToolbarSlot {
    static let move = ToolbarSlot(id: "move", title: "Move", systemImage: "rectangle.and.hand.point.up.left",
                                  help: "Move models on the plate: drag one to place it (M). Esc to stop.", order: 0)
}

/// Move (`ui/move.js`): the toolbar's Move toggle, Model > Move, the M key and Esc. The tool's pointer is the
/// viewer's (`MoveTool`); what a drag does to the plate is `MoveModel`.
@MainActor
final class MoveFeature: Feature {
    weak let host: ViewerModel?
    let model: MoveModel
    @ObservationIgnored private var keys: KeyMonitor?

    init(host: ViewerModel) {
        self.host = host
        model = MoveModel(host: host)
        if host.controller != nil {
            keys = KeyMonitor { [weak self] in self?.key($0) ?? false }
        }
    }

    var toolbarSlots: [ToolbarSlot] { [.move] }

    func registerTools(in toolbar: ToolbarRegistry) {
        guard host?.controller != nil else { return }
        toolbar.registerToggle(.move, isOn: { [model] in model.isOn }, setOn: { [model] in model.setOn($0) })
    }

    /// A model removed, or another loaded: the tool stays on only with a model to move (`partchange`).
    func partDidShow(_ parts: [LoadedPart]) { if model.isOn && !(host?.features[PlatesFeature.self]?.board.hasLive ?? false) { model.setOn(false) } }

    var modifiers: [WindowModifier] {
        [
            // the last model taken off: nothing to move
            .onChange("move-needs-a-model", of: { self.host?.hasPart == true }) { _, has in if !has { self.model.setOn(false) } },
        ]
    }

    var escapeHandlers: [EscapeHandler] {
        [EscapeHandler(id: "move", priority: EscapeHandler.windowMode + 10) { self.escape(NSApp.currentEvent) }]
    }

    /// M toggles the tool, Esc leaves it, wherever the focus is but in a text field (`keydown`, `ui/move.js:182-196`);
    /// Esc first closes an open popover (its own handler).
    private func key(_ event: NSEvent) -> Bool {
        guard let host, let controller = host.controller, controller.isInKeyWindow, host.hasPart,
              event.modifierFlags.intersection([.command, .control, .option]).isEmpty else { return false }
        if let r = event.window?.firstResponder, r is NSText || r is NSTextField { return false }
        if event.keyCode == 53 { return escape(event) }           // Esc
        guard event.charactersIgnoringModifiers?.lowercased() == "m", !event.isARepeat else { return false }
        model.toggle()
        return true
    }

    /// Esc leaves Move, unless the Esc is an open popover's to fold (`ui/move.js:190-193`): open now, or folded by
    /// this same key a moment ago (the toolbar's key monitor can run before this one). True when it left Move.
    func escape(_ event: NSEvent?) -> Bool {
        guard model.isOn, host?.toolbar.escapeIsThePopovers(event) != true else { return false }
        model.setOn(false)
        return true
    }

    static func menus(_ f: MoveFeature?) -> [MenuSection] {
        let model = f?.model
        return [MenuSection(menu: .model, order: 2, entries: [
            .divider,
            .check(JS.t("Move"), help: JS.t("Move models on the plate: drag one to place it (M). Esc to stop."),
                   enabled: f?.host?.hasPart == true,
                   isOn: Binding(get: { model?.isOn ?? false }, set: { model?.setOn($0) })),
        ])]
    }
}
