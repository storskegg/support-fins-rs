import Observation
import SwiftUI

/// A window's features: one instance of each type, and the contributions composed in their fixed order.
@MainActor
final class FeatureSet {
    let types: [any Feature.Type]
    private(set) var instances: [any Feature] = []
    /// The features hold their window (`unowned`), and the menu bar can outlive it (the focused value of a window
    /// that just closed), so a lookup answers nil once the window is gone: the menus then show with nothing to act on.
    private weak var host: ViewerModel?

    /// `host` is not finished when the set is made (it makes its own set), so the features must not call back
    /// into it beyond reading its shared models.
    init(types: [any Feature.Type] = Features.all, host: ViewerModel) {
        self.types = types
        self.host = host
        instances = types.map { Self.make($0, host: host) }
        for f in instances { f.registerTools(in: host.toolbar) }
        host.toolbar.declare(toolbarSlots)
    }

    private static func make<F: Feature>(_ type: F.Type, host: ViewerModel) -> any Feature { F(host: host) }

    subscript<F: Feature>(_ type: F.Type) -> F? { host == nil ? nil : instances.first { $0 is F } as? F }

    // MARK: composition

    var overlays: [WindowOverlay] { sorted(instances.flatMap(\.overlays), by: \.order) }
    var modifiers: [WindowModifier] { instances.flatMap(\.modifiers) }
    var windowToolbarItems: [WindowToolbarItem] { sorted(instances.flatMap(\.windowToolbarItems), by: \.order) }
    var toolbarSlots: [ToolbarSlot] { sorted(instances.flatMap(\.toolbarSlots), by: \.order) }
    var statusLines: [StatusLine] { sorted(instances.flatMap(\.statusLines), by: \.order) }

    func inspectorSections(_ slot: InspectorSlot) -> [InspectorSection] {
        sorted(instances.flatMap(\.inspectorSections).filter { $0.slot == slot }, by: \.order)
    }

    /// The Esc cascade: highest priority first, stopping at the first handler that takes the key.
    var escapeHandlers: [EscapeHandler] { sorted(instances.flatMap(\.escapeHandlers), by: { -$0.priority }) }

    @discardableResult
    func escape() -> Bool {
        for handler in escapeHandlers where handler.handle() { return true }
        return false
    }

    func partWillShow() { instances.forEach { $0.partWillShow() } }
    func partDidShow(_ parts: [LoadedPart]) { instances.forEach { $0.partDidShow(parts) } }

    // MARK: menus

    /// The menus the features name, in menu order. Static: the menu bar lists them before any window exists.
    static func menuIDs(_ types: [any Feature.Type] = Features.all) -> [MenuID] {
        var seen = Set<String>(), ids: [MenuID] = []
        for section in types.flatMap({ sections($0, nil) }) where seen.insert(section.menu.title).inserted { ids.append(section.menu) }
        return sorted(ids, by: \.order)
    }

    /// One menu's entries: the sections of every feature, in section order, for the focused window's `set` (nil when
    /// no window is focused).
    static func entries(of menu: MenuID, types: [any Feature.Type] = Features.all, in set: FeatureSet?) -> [MenuEntry] {
        let sections = types.flatMap { sections($0, set) }.filter { $0.menu.title == menu.title }
        return sorted(sections, by: \.order).flatMap(\.entries)
    }

    /// The whole menu bar as lines, for a snapshot test: "View", then its items.
    static func menuSnapshot(types: [any Feature.Type] = Features.all, in set: FeatureSet?) -> [String] {
        menuIDs(types).flatMap { id in [id.title] + entries(of: id, types: types, in: set).flatMap(\.snapshot).map { "  " + $0 } }
    }

    private static func sections<F: Feature>(_ type: F.Type, _ set: FeatureSet?) -> [MenuSection] { F.menus(set?[F.self]) }
}

/// A stable sort: equal keys keep their given order (the feature-list order).
private func sorted<T, K: Comparable>(_ items: [T], by key: (T) -> K) -> [T] {
    items.enumerated().sorted { (key($0.element), $0.offset) < (key($1.element), $1.offset) }.map(\.element)
}
private func sorted<T, K: Comparable>(_ items: [T], by key: KeyPath<T, K>) -> [T] { sorted(items) { $0[keyPath: key] } }

// MARK: the menu bar

struct FeaturesKey: FocusedValueKey { typealias Value = FeatureSet }
extension FocusedValues {
    /// The focused window's features: the one focused value the menus read.
    var features: FeatureSet? {
        get { self[FeaturesKey.self] }
        set { self[FeaturesKey.self] = newValue }
    }
}

/// The menu bar, composed from `Features.all`: the items after File > New and at the top of View (the system's
/// menus, `MenuID.group`), then every menu of the app's own the features name in `MenuID.order` (Model, then any a
/// feature adds), so the bar shows what `FeatureSet.menuSnapshot` lists.
/// SwiftUI cannot build `Commands` from a list, hence the fixed slots: `FeatureSetTests` checks there are enough.
struct FeatureCommands: Commands {
    /// How many menus of the app's own the bar can show (Model takes one).
    static let menuSlots = 5

    @FocusedValue(\.features) private var features

    /// The entries are made in a view's body, not here, so what they read (the exporter, the badges) is observed
    /// by that view: the focused value (the window's `FeatureSet`) does not change when they do.
    private func menu(_ id: MenuID) -> some View { FeatureMenuContent(menu: id, features: features) }
    private static var menus: [MenuID] { FeatureSet.menuIDs().filter(\.isOwnMenu) }

    var body: some Commands {
        let menus = Self.menus
        CommandGroup(after: .newItem) { menu(.fileExport) }
        CommandGroup(after: .undoRedo) { menu(.edit) }
        CommandGroup(before: .toolbar) { menu(.view) }
        if menus.count > 0 { CommandMenu(menus[0].title) { menu(menus[0]) } }
        if menus.count > 1 { CommandMenu(menus[1].title) { menu(menus[1]) } }
        if menus.count > 2 { CommandMenu(menus[2].title) { menu(menus[2]) } }
        if menus.count > 3 { CommandMenu(menus[3].title) { menu(menus[3]) } }
        if menus.count > 4 { CommandMenu(menus[4].title) { menu(menus[4]) } }
    }
}

/// One menu of the menu bar for the focused window's features.
private struct FeatureMenuContent: View {
    let menu: MenuID
    let features: FeatureSet?
    var body: some View { MenuEntriesView(entries: FeatureSet.entries(of: menu, in: features)) }
}
