import SwiftUI

/// The inspector's tabs (app-only: the web's options panel is one long column, docs/AGENTS.md "Divergences"). The
/// search box sits above them; a search shows the matches of every tab at once (`InspectorView`).
enum InspectorTab: String, CaseIterable, Identifiable {
    /// The stats card (File ... Analysis, Add model, Remove model) and the report links. The status card (the Fins line
    /// and the notes) is pinned above the tabs.
    case part
    /// Printer, Placement, Tines, Cutouts, and what features add after them (Remove fins, the Draw controls).
    case supports
    /// More settings: Hold the part, Tall parts, Where supports go, Display.
    case more
    /// Plate ties (issue #50).
    case plateTies

    var id: String { rawValue }

    var title: String {
        switch self {
        case .part: JS.t("Part")
        case .supports: JS.t("Supports")
        case .more: JS.t("More")
        case .plateTies: JS.t("Plate ties")
        }
    }

    /// The tabs a search reaches: the Part tab holds no setting.
    static let searchable: [InspectorTab] = [.supports, .more, .plateTies]
}

extension SearchEntry {
    /// The tab the row sits in: More settings' groups in More, Plate ties in its own, the rest (Printer, Placement,
    /// Tines, Cutouts, Remove fins, Draw) in Supports.
    var tab: InspectorTab {
        if blocks.contains(.plateTies) { return .plateTies }
        if blocks.contains(.more) { return .more }
        return .supports
    }
}

extension SettingsSearch {
    /// The rows the current search shows in `tab` (matches, and the switches shown for them), in the panel's order;
    /// empty while the box is empty.
    func hits(in tab: InspectorTab) -> [String] {
        guard let result else { return [] }
        let ctx = context()
        return SearchCatalog.allRows.filter { $0.tab == tab && result.show.contains($0.id) && $0.shown(ctx) }.map(\.id)
    }
}
