import SwiftUI

/// The badges of the floating view toolbar that other tracks fill in (`ui/toolbar.js`, UI-MAP section 5).
/// Order is fixed: Undo, Redo, |, Rotate, Suggest, Strength, View. A slot nobody has registered shows dimmed and inert.
enum ToolbarSlot: String, CaseIterable, Identifiable {
    case rotate, suggest, strength, view
    var id: String { rawValue }
    var title: LocalizedStringKey {
        switch self {
        case .rotate: "Rotate"
        case .suggest: "Suggest"
        case .strength: "Strength"
        case .view: "View"
        }
    }
    var systemImage: String {
        switch self {
        case .rotate: "rotate.3d"
        case .suggest: "wand.and.stars"
        case .strength: "arrow.up.and.down.and.arrow.left.and.right"
        case .view: "eye"
        }
    }
    var help: LocalizedStringKey {
        switch self {
        case .rotate: "Turn the part: 90° steps, Reset, Lay a face flat and the rotate rings."
        case .suggest: "Rank the ways this part could sit on the bed."
        case .strength: "Strength arrow: which way is the part loaded in use?"
        case .view: "Camera views, the mouse preset and the print layers around the part."
        }
    }
}

/// What a track registers for one slot: whether the badge is lit (e.g. a load is set) and its popover.
struct ToolbarBadge {
    var isLit: () -> Bool
    var popover: () -> AnyView

    init<Content: View>(isLit: @escaping () -> Bool = { false }, @ViewBuilder popover: @escaping () -> Content) {
        self.isLit = isLit
        self.popover = { AnyView(popover()) }
    }
}

/// The badge-slot API. Track D plugs its Suggest and Strength popovers in with
/// `registry.register(.suggest) { SuggestPopover(...) }`; one popover is open at a time.
@MainActor @Observable
final class ToolbarRegistry {
    private(set) var badges: [ToolbarSlot: ToolbarBadge] = [:]
    /// The slot whose popover is open, if any. Setting it folds the others.
    var open: ToolbarSlot?
    /// Short text shown over a badge for a moment (the web's `#drag-angle` over Rotate).
    var flash: (slot: ToolbarSlot, text: String)?

    func register(_ slot: ToolbarSlot, badge: ToolbarBadge) { badges[slot] = badge }

    func register<Content: View>(_ slot: ToolbarSlot, isLit: @escaping () -> Bool = { false },
                                 @ViewBuilder popover: @escaping () -> Content) {
        badges[slot] = ToolbarBadge(isLit: isLit, popover: popover)
    }

    func unregister(_ slot: ToolbarSlot) {
        badges[slot] = nil
        if open == slot { open = nil }
    }

    func toggle(_ slot: ToolbarSlot) { open = open == slot ? nil : slot }
    func fold() { open = nil }

    private var flashGeneration = 0

    /// Show `text` above a badge, folding away after `seconds` (never, when nil: a drag holds it and the next
    /// call replaces it).
    func flash(_ text: String, over slot: ToolbarSlot, seconds: Double? = 1.2) {
        flash = (slot, text)
        flashGeneration += 1
        guard let seconds else { return }
        let generation = flashGeneration
        Task { try? await Task.sleep(for: .seconds(seconds)); if flashGeneration == generation { flash = nil } }
    }
}
