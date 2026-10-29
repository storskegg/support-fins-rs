import SwiftUI

/// Settings search (`ui/search.js`, local issue 42): the box that narrows the inspector to the settings whose name
/// matches (Edit > Search Settings, ⌥⌘F, focuses it). The matching is `SearchEngine`; the rows hide themselves
/// (`FindRow`, `FindBlock`).
@MainActor
final class SearchFeature: Feature {
    weak let host: ViewerModel?
    let model = SettingsSearch()

    init(host: ViewerModel) {
        self.host = host
        model.context = { [weak self] in self?.context() ?? SearchContext() }
    }

    /// The state the rows depend on: the settings and what Draw and Remove fins offer.
    func context() -> SearchContext {
        guard let host else { return SearchContext() }
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
        [InspectorSection("search-field", slot: .top, order: -20) { SearchField(search: self.model) },
         InspectorSection("search-note", slot: .top, order: -10) { SearchEmptyNote(search: self.model) }]
    }

    static func menus(_ f: SearchFeature?) -> [MenuSection] {
        [MenuSection(menu: .edit, order: 30, entries: [
            .divider,
            .item(JS.t("Search Settings"), shortcut: "f", modifiers: [.command, .option], help: JS.t("Search settings"),
                  enabled: f?.host?.hasPart == true) { f?.model.requestFocus() },
        ])]
    }
}

/// The window with the model in the environment for the rows, and the search re-run whenever what shows or hides a row
/// changes (the page's `MutationObserver`).
private struct SearchHost: View {
    let feature: SearchFeature
    let content: AnyView

    var body: some View {
        content
            .environment(feature.model)
            .onChange(of: feature.context()) { feature.model.refresh() }
    }
}

/// The box at the top of the options panel (`#opts-search`): Esc in it clears it; Edit > Search Settings focuses it.
struct SearchField: View {
    @Bindable var search: SettingsSearch
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
            TextField("Search settings", text: $search.query)
                .lineLimit(1)
                .textFieldStyle(.plain)
                .focused($focused)
                .autocorrectionDisabled()
                .onKeyPress(.escape) { search.clear() ? .handled : .ignored }
                .accessibilityIdentifier("opts-search")
            if !search.query.isEmpty {
                Button { search.query = "" } label: { Image(systemName: "xmark.circle.fill") }
                    .buttonStyle(.plain).foregroundStyle(.secondary)
                    .accessibilityLabel("Clear")
            }
        }
        .padding(6)
        .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 6))
        .onChange(of: search.focusRequests) { focused = true }
    }
}
