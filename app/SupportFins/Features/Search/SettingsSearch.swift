import Observation
import SwiftUI

/// The search box's state and the last result, which the inspector's rows read (`FindRow`, `FindBlock`).
@MainActor
@Observable
final class SettingsSearch {
    /// The text in the box (`#opts-search`). Setting it runs the search.
    var query = "" { didSet { if query != oldValue { refresh() } } }
    private(set) var result: SearchEngine.Result?
    /// Bumped by Edit > Search Settings: the field takes the focus.
    private(set) var focusRequests = 0

    @ObservationIgnored private var engine = SearchEngine()
    /// The window's state the rows depend on; set by `SearchFeature`.
    @ObservationIgnored var context: () -> SearchContext = { SearchContext() }

    var isSearching: Bool { result != nil }

    /// Re-run the current query (the page does it after any change that hides or shows a row).
    func refresh() {
        result = engine.run(query: query, context())
    }

    func requestFocus() { focusRequests += 1 }

    /// `Esc` in the box: clear it. True when there was something to clear.
    @discardableResult
    func clear() -> Bool {
        guard !query.isEmpty else { return false }
        query = ""
        return true
    }

    // MARK: what the rows read

    /// Is the row on show? Everything is, while the box is empty.
    func shows(_ id: String) -> Bool { result.map { $0.show.contains(id) } ?? true }
    /// Is the row one of the matches (or a switch shown for one)? Marked in the panel.
    func isHit(_ id: String) -> Bool { result?.show.contains(id) ?? false }
    /// Is the section or group on show? The ones holding no match are hidden.
    func shows(block: SearchBlock) -> Bool { result.map { $0.blocks.contains(block) } ?? true }
    /// A section opened for a search is not the user opening it: the user's own state is left as it was and comes back
    /// when the box is cleared (`settings.js` skips saving while `.searching`).
    func expanded(_ block: SearchBlock, user: Bool) -> Bool { isSearching ? shows(block: block) : user }
    var noMatch: Bool { result?.none == true }

    /// The disclosure binding of a section: open for a search that found something in it, the user's own otherwise.
    func expandedBinding(_ block: SearchBlock, user: Binding<Bool>) -> Binding<Bool> {
        Binding(get: { self.expanded(block, user: user.wrappedValue) },
                set: { if !self.isSearching { user.wrappedValue = $0 } })
    }
}

/// A row of the inspector: shown while it matches (or the box is empty), marked as a hit while searching
/// (`#fin-opts.searching .hit`: a bar on its left).
struct FindRow<Content: View>: View {
    let id: String
    /// A row in a section's header is never hidden, only marked (`:not(summary *)`).
    let hides: Bool
    @ViewBuilder var content: Content
    @Environment(SettingsSearch.self) private var search: SettingsSearch?

    init(_ id: String, hides: Bool = true, @ViewBuilder content: () -> Content) {
        self.id = id
        self.hides = hides
        self.content = content()
    }

    var body: some View {
        if !hides || (search?.shows(id) ?? true) {
            content
                .overlay(alignment: .leading) {
                    if search?.isHit(id) == true {
                        Rectangle().fill(Color.accentColor).frame(width: 3).offset(x: -10)
                    }
                }
        }
    }
}

/// A section or group: on show unless a search found nothing in it.
struct FindBlock<Content: View>: View {
    let block: SearchBlock
    @ViewBuilder var content: Content
    @Environment(SettingsSearch.self) private var search: SettingsSearch?

    init(_ block: SearchBlock, @ViewBuilder content: () -> Content) {
        self.block = block
        self.content = content()
    }

    var body: some View {
        if search?.shows(block: block) ?? true { content }
    }
}

/// "No settings match", under the box (`#opts-search-empty`).
struct SearchEmptyNote: View {
    let search: SettingsSearch

    var body: some View {
        if search.noMatch {
            Text("No settings match").font(.caption).foregroundStyle(.secondary).accessibilityIdentifier("opts-search-empty")
        }
    }
}
