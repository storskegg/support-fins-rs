import Foundation
import Testing

@testable import Support_Fins

/// The inspector's tabs (`InspectorTab`): which tab each searchable row sits in, and what a search shows of each tab.
@Suite("Inspector tabs")
@MainActor
struct InspectorTabTests {
    @Test func everyRowSitsInTheTabItsSectionIsIn() {
        let tabs = Dictionary(uniqueKeysWithValues: SearchCatalog.allRows.map { ($0.id, $0.tab) })
        // Printer, Placement, Tines, the basics after Cutouts and Draw: Supports
        for id in ["printer-profile", "nozzle-fld", "layerh-fld", "gap", "iface", "fin-mode-fld", "tines", "tinegrip-fld",
                   "tinedir-fld", "pindepth-fld", "cutout", "augment-toggle", "remove-fins-toggle", "restore-fins",
                   "draw-shape", "draw-back", "draw-remove", "draw-clear"] {
            #expect(tabs[id] == .supports, "\(id)")
        }
        // More settings' four groups: More
        for id in ["rounded-feet-fld", "bed-pad", "pad-gap", "wall-lines", "tall-ribs", "sway", "coverage-fld", "plate-only",
                   "curved-walls-fld", "highlight-small", "show-islands"] {
            #expect(tabs[id] == .more, "\(id)")
        }
        #expect(SearchCatalog.appRows.filter { $0.id.hasPrefix("plate-ties") }.allSatisfy { $0.tab == .plateTies })
        // the Part tab holds no setting
        #expect(!tabs.values.contains(.part))
        #expect(InspectorTab.searchable == [.supports, .more, .plateTies])
    }

    @Test func aSearchListsTheRowsOnShowInEachTabInThePanelsOrder() {
        let search = SettingsSearch()
        search.context = { var c = SearchContext(); c.tiesOn = true; return c }
        #expect(InspectorTab.allCases.allSatisfy { search.hits(in: $0).isEmpty }, "an empty box shows the tabs as they are")
        // Support gap, the Bed pad that reveals Pad gap (Pad gap itself is hidden outside Custom), Max gap
        search.query = "gap"
        #expect(search.hits(in: .supports) == ["gap"])
        #expect(search.hits(in: .more) == ["bed-pad"])
        #expect(search.hits(in: .plateTies) == ["plate-ties-max-gap"])
        #expect(search.hits(in: .part).isEmpty)
        search.query = "zzzz"
        #expect(InspectorTab.allCases.allSatisfy { search.hits(in: $0).isEmpty })
        search.query = ""
        #expect(search.hits(in: .supports).isEmpty)
    }
}
