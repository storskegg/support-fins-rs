import SwiftUI

/// Settings search (`ui/search.js`, local issue 42): the box that narrows the inspector to the settings whose name
/// matches (Edit > Search Settings, ⌥⌘F, focuses it). The matching is `SearchEngine`; the rows hide themselves
/// (`FindRow`, `FindBlock`).
@MainActor
final class SearchFeature: Feature {
    unowned let host: ViewerModel
    let model = SettingsSearch()

    init(host: ViewerModel) {
        self.host = host
        model.context = { [weak self] in self?.context() ?? SearchContext() }
    }

    /// The state the rows depend on: the settings and what Draw and Remove fins offer.
    func context() -> SearchContext {
        var c = SearchContext(settings: host.session.settings)
        if let draw = host.features[DrawFeature.self]?.model {
            c.augmentOffered = draw.augmentOffered
            c.drawControls = draw.controlsShown
            c.drawHint = draw.hintShown
            c.drawStarted = draw.started
            c.drawSelected = draw.selectedID != nil
        }
        if let removal = host.wiring?.removal {
            c.removeControls = removal.controlsShown
            c.canRestore = removal.canRestore
        }
        return c
    }

    var modifiers: [WindowModifier] {
        [WindowModifier("settings-search") { content in AnyView(SearchHost(feature: self, content: content)) }]
    }

    var inspectorSections: [InspectorSection] {
        [InspectorSection("search-note", slot: .top, order: -10) { SearchEmptyNote(search: self.model) }]
    }

    static func menus(_ f: SearchFeature?) -> [MenuSection] {
        [MenuSection(menu: .edit, order: 30, entries: [
            .divider,
            .item("Search Settings", shortcut: "f", modifiers: [.command, .option], help: "Search settings",
                  enabled: f?.host.hasPart == true) { f?.model.requestFocus() },
        ])]
    }
}

/// The window with the search field (`#opts-search`) in its toolbar, the model in the environment for the rows, and the
/// search re-run whenever what shows or hides a row changes (the page's `MutationObserver`).
private struct SearchHost: View {
    let feature: SearchFeature
    let content: AnyView
    @FocusState private var focused: Bool

    var body: some View {
        @Bindable var model = feature.model
        content
            .searchable(text: $model.query, placement: .toolbar, prompt: "Search settings")
            .searchFocused($focused)
            .environment(feature.model)
            .onChange(of: feature.model.focusRequests) { focused = true }
            .onChange(of: feature.context()) { feature.model.refresh() }
    }
}
