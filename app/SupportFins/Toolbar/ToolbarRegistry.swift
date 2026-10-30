import AppKit
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
    /// A tool badge (Section, `addBadge` with `setOn`, `ui/toolbar.js:134-142`) is a tool that is on or off: the first
    /// click turns it on and opens its popover; opening another popover leaves it on (its badge stays lit); a click on
    /// the lit badge brings its popover back, and with the popover open turns it off. A panel badge has none.
    var tool: (isOn: () -> Bool, setOn: (Bool) -> Void)?
    /// A toggle badge (Move, `addBadge` with `setOn` and no popover, `ui/toolbar.js:134-136`): a tool with nothing to
    /// fold out, so a click just turns it on or off and any open popover stays as it is.
    var toggle: (isOn: () -> Bool, setOn: (Bool) -> Void)?
    /// A plain button badge (Arrange, `ui/arrange.js`): a click runs `action`; there is nothing to fold out and nothing
    /// to switch. `isShown` hides it (Arrange is only there with several models), `isEnabled` greys it.
    var action: (() -> Void)?
    var isShown: () -> Bool = { true }
    var isEnabled: () -> Bool = { true }
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

    func register<Content: View>(_ slot: ToolbarSlot, isLit: @escaping () -> Bool = { false },
                                 detail: @escaping () -> String = { "" }, @ViewBuilder popover: @escaping () -> Content) {
        var badge = ToolbarBadge(isLit: isLit, popover: popover)
        badge.detail = detail
        badges[slot] = badge
    }

    /// Register a tool badge: lit while the tool is on, with its popover.
    func registerTool<Content: View>(_ slot: ToolbarSlot, isOn: @escaping () -> Bool, setOn: @escaping (Bool) -> Void,
                                     @ViewBuilder popover: @escaping () -> Content) {
        var badge = ToolbarBadge(isLit: isOn, popover: popover)
        badge.tool = (isOn, setOn)
        badges[slot] = badge
    }

    /// Register a toggle badge: lit while the tool is on; it has no popover.
    func registerToggle(_ slot: ToolbarSlot, isOn: @escaping () -> Bool, setOn: @escaping (Bool) -> Void) {
        var badge = ToolbarBadge(isLit: isOn) { EmptyView() }
        badge.toggle = (isOn, setOn)
        badges[slot] = badge
    }

    /// Register a plain button badge (a click runs `action`; see `ToolbarBadge.action`).
    func registerAction(_ slot: ToolbarSlot, isShown: @escaping () -> Bool = { true }, isEnabled: @escaping () -> Bool = { true },
                        action: @escaping () -> Void) {
        var badge = ToolbarBadge { EmptyView() }
        badge.action = action
        badge.isShown = isShown
        badge.isEnabled = isEnabled
        badges[slot] = badge
    }

    func unregister(_ slot: ToolbarSlot) {
        badges[slot] = nil
        if open == slot { open = nil }
    }

    func toggle(_ slot: ToolbarSlot) {
        if let a = badges[slot]?.action { a(); return }
        if let t = badges[slot]?.toggle { t.setOn(!t.isOn()); return }
        guard let tool = badges[slot]?.tool else { open = open == slot ? nil : slot; return }
        if !tool.isOn() {
            tool.setOn(true)
            open = tool.isOn() ? slot : nil
        } else if open != slot {
            open = slot
        } else {
            tool.setOn(false)
            open = nil
        }
    }
    func fold() { open = nil }

    /// The Esc that last folded a popover (its event's timestamp). The web's Esc listeners all see the same key:
    /// Move's runs first (capture) and leaves the key to an open popover (`ui/move.js:190-193`), the toolbar's folds it
    /// (`ui/toolbar.js:106-114`). Here the toolbar's monitor may run before Move's, so Move asks whether this very Esc
    /// has just folded a popover, rather than whether one is open now.
    @ObservationIgnored private var escapeFoldTime: TimeInterval?

    /// Esc folds the open popover (`EscapeFolds`); remembered for `escapeFoldedPopover(_:)`.
    func fold(byEscape e: NSEvent) {
        guard open != nil else { return }
        escapeFoldTime = e.timestamp
        fold()
    }

    /// Whether `e` is the Esc that folded a popover, or a popover is open for it to fold: a handler that must leave
    /// such an Esc to the popover returns false for it.
    func escapeIsThePopovers(_ e: NSEvent?) -> Bool {
        if open != nil { return true }
        guard let e, e.type == .keyDown, e.keyCode == 53, let t = escapeFoldTime else { return false }
        return e.timestamp == t
    }

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
