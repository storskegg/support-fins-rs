import SwiftUI

// What a feature can contribute to the window (see `Feature`). Every contribution is plain data: the window
// (`ContentView`) and the menu bar (`FeatureCommands`) compose them from `Features.all` in a fixed order, and
// `FeatureSetTests` pins that order. `order` sorts ascending; ties keep the feature-list order.

/// A layer over the viewport. Layers stack in `order`, the last on top. Each fills the window and puts its content at
/// `alignment`; an empty layer takes no clicks.
struct WindowOverlay: Identifiable {
    let id: String
    var order: Int
    var alignment: Alignment
    /// Draw under the title bar and the toolbar, like the viewport (only the frame-time readout needs it).
    var ignoresSafeArea: Bool
    /// The layer places its content at viewport points (a projected point, the pointer), so it stays left to right
    /// in a right-to-left language: SwiftUI mirrors `.position` and `.offset` there, the viewport does not mirror.
    var inViewportSpace: Bool
    var content: () -> AnyView

    init<V: View>(_ id: String, order: Int, alignment: Alignment = .topLeading, ignoresSafeArea: Bool = false,
                  inViewportSpace: Bool = false, @ViewBuilder content: @escaping () -> V) {
        self.id = id
        self.order = order
        self.alignment = alignment
        self.ignoresSafeArea = ignoresSafeArea
        self.inViewportSpace = inViewportSpace
        self.content = { AnyView(content()) }
    }
}

/// A modifier on the window's content: sheets, drop targets, `.onChange` of the pose or the threshold. Applied in
/// the feature-list order, the first innermost.
struct WindowModifier: Identifiable {
    let id: String
    var apply: (AnyView) -> AnyView

    init(_ id: String, apply: @escaping (AnyView) -> AnyView) {
        self.id = id
        self.apply = apply
    }

    /// `.onChange(of: value())`. `value` is read while the window's body is, so a change of what it reads
    /// re-evaluates the window as it does for any other modifier.
    static func onChange<V: Equatable>(_ id: String, of value: @escaping () -> V, initial: Bool = false,
                                       _ action: @escaping (_ old: V, _ new: V) -> Void) -> WindowModifier {
        WindowModifier(id) { content in AnyView(content.onChange(of: value(), initial: initial) { old, new in action(old, new) }) }
    }
}

/// An item of the window toolbar (the bar over the inspector), between the fixed Overhang / Add fins items and the
/// Inspector switch.
struct WindowToolbarItem: Identifiable {
    let id: String
    var order: Int
    var content: () -> AnyView

    init<V: View>(_ id: String, order: Int, @ViewBuilder content: @escaping () -> V) {
        self.id = id
        self.order = order
        self.content = { AnyView(content()) }
    }
}

/// Where in the inspector a section goes.
enum InspectorSlot: Int, CaseIterable {
    /// Above everything: the stats and status cards.
    case top
    /// Inside the stats card (Part), after Analysis: the web's `part-btns` (Add model, Remove model). The readout
    /// draws these, not the inspector.
    case stats
    /// In the settings block, after Cutouts and before More settings (Remove fins).
    case basics
    /// After the settings block.
    case bottom
}

struct InspectorSection: Identifiable {
    let id: String
    var slot: InspectorSlot
    var order: Int
    var content: () -> AnyView

    init<V: View>(_ id: String, slot: InspectorSlot, order: Int = 0, @ViewBuilder content: @escaping () -> V) {
        self.id = id
        self.slot = slot
        self.order = order
        self.content = { AnyView(content()) }
    }
}

/// A line of the status card, after the card's own notes and before the grams receipt.
struct StatusLine: Identifiable {
    let id: String
    var order: Int
    var text: String
    var color: Color
    /// The 3 px bar on the line's left, for a warning.
    var bar: Color?

    init(_ id: String, order: Int = 0, text: String, color: Color = .secondary, bar: Color? = nil) {
        self.id = id
        self.order = order
        self.text = text
        self.color = color
        self.bar = bar
    }
}

/// A turn in the Esc cascade. Handlers run from the highest `priority` down; the first to return true has
/// consumed the key and the rest do not run. Ties keep the feature-list order.
struct EscapeHandler: Identifiable {
    let id: String
    var priority: Int
    var handle: @MainActor () -> Bool

    /// An armed pointer mode that Esc leaves (lay a face flat, Remove fins).
    static let pointerMode = 100
    /// A mode of the window that Esc ends when no pointer mode is armed.
    static let windowMode = 50
}

// MARK: menus

/// A menu of the menu bar. Features that name the same menu share it; menus come in `order`. A menu macOS already
/// has (File, Edit, View, ...) is never added again: its items go into the system menu at `group` (`fileExport`: after
/// File > New; `edit`: after Undo and Redo; `view`: the top of View, above Show Toolbar and Enter Full Screen). A `CommandMenu("View")` would be a
/// second View menu next to the system's.
struct MenuID: Hashable, Identifiable {
    var title: String
    var order: Int
    /// Where in a system menu the items go; nil for a menu of the app's own (`CommandMenu`).
    var group: SystemGroup?
    var id: String { title }

    /// The places in the system's menus that `FeatureCommands` fills.
    enum SystemGroup: Hashable, CaseIterable {
        /// File, after New.
        case afterNewItem
        /// Edit, after Undo and Redo.
        case afterUndoRedo
        /// View, before the toolbar items.
        case beforeToolbar
        /// Help, after the system's own "Support Fins Help".
        case afterHelp
    }

    /// A menu of the app's own, added to the bar (`CommandMenu`).
    var isOwnMenu: Bool { group == nil }

    /// The menus macOS (and SwiftUI's document app) already put in the bar: a feature menu with one of these titles
    /// must fill a `group` of it, not add a menu of the same name.
    static let systemTitles: Set<String> = ["File", "Edit", "Format", "View", "Window", "Help"]

    static let view = MenuID(title: "View", order: 0, group: .beforeToolbar)
    static let model = MenuID(title: JS.t("Model"), order: 10)
    static let fileExport = MenuID(title: "File", order: 100, group: .afterNewItem)
    /// Items after Undo and Redo in the Edit menu (Remove Model from Plate).
    static let edit = MenuID(title: "Edit", order: 110, group: .afterUndoRedo)
    /// Items after "Support Fins Help" in the Help menu (Calibration Prints, the links).
    static let help = MenuID(title: "Help", order: 200, group: .afterHelp)
}

/// A run of one menu's entries from one feature. Sections of a menu come in `order`, so a feature's items sit
/// where the menu has always had them without the features knowing of each other.
struct MenuSection {
    var menu: MenuID
    var order: Int
    var entries: [MenuEntry]
}

struct MenuButton {
    var title: String
    var shortcut: KeyEquivalent?
    var modifiers: EventModifiers = .command
    var help: String?
    var enabled: Bool
    var action: @MainActor () -> Void
}

struct MenuToggle {
    var title: String
    var help: String?
    var enabled: Bool
    var isOn: Binding<Bool>
}

/// A menu item as data, so the menu bar is composed and its titles are testable.
indirect enum MenuEntry {
    case button(MenuButton)
    case toggle(MenuToggle)
    case divider
    /// A line of text under the items (the Export note).
    case note(String)
    case submenu(String, [MenuEntry])

    static func item(_ title: String, shortcut: KeyEquivalent? = nil, modifiers: EventModifiers = .command,
                     help: String? = nil, enabled: Bool = true,
                     action: @escaping @MainActor () -> Void) -> MenuEntry {
        .button(MenuButton(title: title, shortcut: shortcut, modifiers: modifiers, help: help, enabled: enabled, action: action))
    }

    static func check(_ title: String, help: String? = nil, enabled: Bool = true, isOn: Binding<Bool>) -> MenuEntry {
        .toggle(MenuToggle(title: title, help: help, enabled: enabled, isOn: isOn))
    }

    /// The entry as one line of a snapshot: its title, "-" for a divider, "> " for a submenu's items.
    var snapshot: [String] {
        switch self {
        case .button(let b): [b.title + (b.shortcut.map { " [\(b.modifiers.symbols)\($0.character)]" } ?? "")]
        case .toggle(let t): [t.title]
        case .divider: ["-"]
        case .note(let text): ["(\(text))"]
        case .submenu(let title, let entries): [title] + entries.flatMap(\.snapshot).map { "> " + $0 }
        }
    }
}

/// A contribution's content, called in a body of its own. What the content reads (`host.wiring?.exporter`, ...) is
/// then observed by this view, which updates wherever SwiftUI hosts it: a toolbar item or a menu is not rebuilt with
/// the window's body, so content called there would keep what it showed first (an Export menu disabled for good).
struct ContributionView: View {
    let content: () -> AnyView
    var body: some View { content() }
}

/// Renders entries as menu content, in the menu bar and in the window toolbar's Export menu.
struct MenuEntriesView: View {
    var entries: [MenuEntry]

    var body: some View {
        ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in MenuEntryView(entry: entry) }
    }
}

private struct MenuEntryView: View {
    var entry: MenuEntry

    var body: some View {
        switch entry {
        case .button(let b):
            Button(b.title, action: b.action)
                .keyboardShortcut(b.shortcut.map { KeyboardShortcut($0, modifiers: b.modifiers) })
                .help(b.help ?? "")
                .disabled(!b.enabled)
        case .toggle(let t):
            Toggle(t.title, isOn: t.isOn)
                .help(t.help ?? "")
                .disabled(!t.enabled)
        case .divider:
            Divider()
        case .note(let text):
            Text(text)
        case .submenu(let title, let entries):
            Menu(title) { MenuEntriesView(entries: entries) }
        }
    }
}

extension EventModifiers {
    /// Mac order (control, option, shift, command); empty for the plain command key, which a snapshot leaves unsaid.
    var symbols: String {
        guard self != .command else { return "" }
        return [(Self.control, "⌃"), (.option, "⌥"), (.shift, "⇧"), (.command, "⌘")].filter { contains($0.0) }.map(\.1).joined()
    }
}
