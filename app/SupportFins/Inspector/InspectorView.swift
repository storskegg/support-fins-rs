import SwiftUI
import Viewer

/// The inspector: the search box and the status card on top (the header, on show in every tab), then four tabs
/// (`InspectorTab`, app-only: the web's options panel is one long column). Part holds the stats card; Supports the web's settings block in its order (docs/UI-MAP.md
/// section 4: Printer, Placement, Tines, the basics); More the four groups of More settings; Plate ties its section.
/// The settings are the session's (`PartSession.settings`): a change is one undoable step.
///
/// - Each tab is a form of its own, so each scrolls by itself. The Part tab stays alive while another shows (hidden, out
///   of the accessibility tree and the key loop): its readout analyses the part for the viewer's midair cones.
/// - A search shows the matches of every tab in one form, each tab's after a button that shows them in their tab: the
///   box is cleared, the tab selected, and the first match scrolled to and marked for a moment.
/// - The tab is the window's (`SceneStorage`), like the open sections.
struct InspectorView: View {
    @Bindable var controller: ViewerController
    @Binding var settings: FinSettings
    /// The features' sections (`Feature.inspectorSections`) by slot: above the tabs (the search box, the status card),
    /// the Part tab (an empty one keeps M4a's Part block in place of the stats card), the Supports tab after Cutouts, and the Plate ties tab.
    var header: [InspectorSection] = []
    var part: [InspectorSection] = []
    var basics: [InspectorSection] = []
    var plateTies: [InspectorSection] = []
    @SceneStorage(SettingsStore.Key.sections) private var sections = SectionState()
    @SceneStorage(Self.tabKey) private var tab = InspectorTab.part
    @Environment(SettingsSearch.self) private var search: SettingsSearch?
    /// A row to scroll to once its tab shows (a search result shown in its tab).
    @State private var pendingReveal: String?

    /// The window's selected tab.
    static let tabKey = "sf.inspectorTab"

    private var searching: Bool { search?.isSearching == true }
    private var tabs: [InspectorTab] { InspectorTab.allCases.filter { $0 != .plateTies || !plateTies.isEmpty } }

    var body: some View {
        VStack(spacing: 0) {
            if !header.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(header) { $0.content() }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 12)
                .padding(.top, 10)
            }
            if !searching {
                Picker("Inspector", selection: $tab) {
                    ForEach(tabs) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .padding(.horizontal, 12)
                .padding(.top, 8)
            }
            ZStack {
                // alive in every tab: the readout analyses the part for the viewer (its midair cones)
                partForm.shown(!searching && tab == .part)
                if searching {
                    searchForm
                } else if tab != .part {
                    ScrollViewReader { proxy in
                        Form { content(tab) }
                            .formStyle(.grouped)
                            .task(id: pendingReveal) { await reveal(with: proxy) }
                    }
                    .id(tab)    // a form of its own per tab: each scrolls by itself
                }
            }
        }
        .onChange(of: settings) { _, new in SettingsStore().save(new) }
        // Highlight small overhangs paints the part, never rebuilds the fins (`web/ui/settings.js:386-392`)
        .onChange(of: settings.highlightSmall, initial: true) { _, on in controller.highlightSmall = on }
        // Show midair starts draws the red cones only (`web/ui/islands.js:30-34`)
        .onChange(of: settings.showIslands, initial: true) { _, on in controller.showsIslands = on }
        // a tab that is gone (no Plate ties section) falls back to Part
        .onChange(of: tabs, initial: true) { _, all in if !all.contains(tab) { tab = .part } }
    }

    // MARK: the tabs

    private var partForm: some View {
        Form {
            ForEach(part) { $0.content() }
            PartSections(controller: controller, withPartBlock: part.isEmpty)
        }
        .formStyle(.grouped)
    }

    @ViewBuilder private func content(_ tab: InspectorTab) -> some View {
        switch tab {
        case .part:
            EmptyView()
        case .supports:
            Section {
                PrinterSection(settings: $settings, open: $sections.printer)
                PlacementSection(settings: $settings)
                TinesSection(settings: $settings, open: $sections.tines)
                FindRow("cutout") {
                    Picker("Cutouts", selection: $settings.cutout) {
                        Text("None").tag(FinSettings.Cutout.none)
                        Text("Diamond").tag(FinSettings.Cutout.diamond)
                        Text("Triangle").tag(FinSettings.Cutout.triangle)
                        Text("Arch").tag(FinSettings.Cutout.arch)
                        Text("Lattice").tag(FinSettings.Cutout.lattice)
                        Text("Honeycomb").tag(FinSettings.Cutout.honeycomb)
                    }
                    .tipCard(pic: "cutout", Tip.cutout)
                }
                ForEach(basics) { $0.content() }
            }
        case .more:
            MoreSettingsSection(settings: $settings, open: $sections.more, folds: false)
            #if DEBUG
            // the last row of the form, not a bar over it: a `safeAreaInset` bar had no background, so the rows
            // scrolled under it and a click aimed at the one beneath landed on the bar
            if !searching {
                Section {
                    Toggle("Benchmark orbit", isOn: Binding(get: { controller.benchmarking }, set: { controller.setBenchmark($0) }))
                }
            }
            #endif
        case .plateTies:
            ForEach(plateTies) { $0.content() }
        }
    }

    // MARK: search

    /// Every tab's matches, each tab's after a button that shows them in their tab.
    private var searchForm: some View {
        Form {
            ForEach(InspectorTab.searchable) { t in
                if let first = search?.hits(in: t).first {
                    Section {
                        Button { show(first, in: t) } label: {
                            Label(t.title, systemImage: "arrow.forward.circle")
                        }
                        .buttonStyle(.link)
                        .help(JS.t("Show in its tab"))
                        .accessibilityIdentifier("search-tab-\(t.rawValue)")
                    }
                    content(t)
                }
            }
        }
        .formStyle(.grouped)
    }

    /// A search result shown in its tab: clear the box, select the tab, open the section the row sits in, then scroll
    /// to the row (`pendingReveal`) and mark it for a moment.
    private func show(_ id: String, in t: InspectorTab) {
        let blocks = SearchCatalog.byID[id]?.blocks ?? []
        if blocks.contains(.printer) { sections.printer = true }
        if blocks.contains(.tines) { sections.tines = true }
        search?.clear()
        tab = t
        search?.revealed = id
        pendingReveal = id
    }

    private func reveal(with proxy: ScrollViewProxy) async {
        guard let id = pendingReveal else { return }
        // the new tab's form lays its rows out first
        try? await Task.sleep(for: .milliseconds(80))
        guard !Task.isCancelled else { return }
        withAnimation { proxy.scrollTo(id, anchor: .center) }
        try? await Task.sleep(for: .seconds(2))
        guard !Task.isCancelled else { return }
        if search?.revealed == id { search?.revealed = nil }
        pendingReveal = nil
    }
}

private extension View {
    /// On show, or kept alive but hidden: invisible, not clickable, out of the accessibility tree and the key loop.
    func shown(_ on: Bool) -> some View {
        opacity(on ? 1 : 0)
            .allowsHitTesting(on)
            .accessibilityHidden(!on)
            .disabled(!on)
    }
}
