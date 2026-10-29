import SwiftUI

/// A badge slot of the floating view toolbar (`ui/toolbar.js`, UI-MAP section 5). A feature declares its slots with
/// `Feature.toolbarSlots` (and a static `ToolbarSlot` of its own); the bar shows them after Undo, Redo and the
/// divider in `order`. A slot nobody has registered a badge for shows dimmed and inert.
struct ToolbarSlot: Hashable, Identifiable {
    let id: String
    var title: LocalizedStringKey
    var systemImage: String
    var help: LocalizedStringKey
    var order: Int

    static func == (a: ToolbarSlot, b: ToolbarSlot) -> Bool { a.id == b.id }
    func hash(into hasher: inout Hasher) { hasher.combine(id) }
}

extension ToolbarSlot: @unchecked Sendable {}   // immutable text and numbers

/// What a track registers for one slot: whether the badge is lit (e.g. a load is set) and its popover.
struct ToolbarBadge {
    var isLit: () -> Bool
    var popover: () -> AnyView
    /// A toggle badge (Move, `addBadge` with `setOn` and no popover, `ui/toolbar.js:134-136`): a tool with nothing to
    /// fold out, so a click just turns it on or off and any open popover stays as it is.
    var toggle: (isOn: () -> Bool, setOn: (Bool) -> Void)?
    /// Text after the badge's title (Scale: the percent while it is not 100, `#scale-now`).
    var detail: () -> String = { "" }

    init<Content: View>(isLit: @escaping () -> Bool = { false }, @ViewBuilder popover: @escaping () -> Content) {
        self.isLit = isLit
        self.popover = { AnyView(popover()) }
    }
}

/// The badge-slot API. A feature plugs its popover in with `registry.register(.suggest) { SuggestPopover(...) }`
/// (from `Feature.registerTools`); one popover is open at a time.
@MainActor @Observable
final class ToolbarRegistry {
    private(set) var badges: [ToolbarSlot: ToolbarBadge] = [:]
    /// The slots the bar shows, in order (`declare`).
    private(set) var slots: [ToolbarSlot] = []
    /// The slot whose popover is open, if any. Setting it folds the others.
    var open: ToolbarSlot?
    /// Short text shown over a badge for a moment (the web's `#drag-angle` over Rotate).
    var flash: (slot: ToolbarSlot, text: String)?

    /// Show these slots, in the given order (the features' `toolbarSlots`, sorted).
    func declare(_ slots: [ToolbarSlot]) { self.slots = slots }

    func register(_ slot: ToolbarSlot, badge: ToolbarBadge) { badges[slot] = badge }

    func register<Content: View>(_ slot: ToolbarSlot, isLit: @escaping () -> Bool = { false },
                                 detail: @escaping () -> String = { "" }, @ViewBuilder popover: @escaping () -> Content) {
        var badge = ToolbarBadge(isLit: isLit, popover: popover)
        badge.detail = detail
        badges[slot] = badge
    }

    /// Register a toggle badge: lit while the tool is on; it has no popover.
    func registerToggle(_ slot: ToolbarSlot, isOn: @escaping () -> Bool, setOn: @escaping (Bool) -> Void) {
        var badge = ToolbarBadge(isLit: isOn) { EmptyView() }
        badge.toggle = (isOn, setOn)
        badges[slot] = badge
    }

    func unregister(_ slot: ToolbarSlot) {
        badges[slot] = nil
        if open == slot { open = nil }
    }

    func toggle(_ slot: ToolbarSlot) {
        if let t = badges[slot]?.toggle { t.setOn(!t.isOn()); return }
        open = open == slot ? nil : slot
    }
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
