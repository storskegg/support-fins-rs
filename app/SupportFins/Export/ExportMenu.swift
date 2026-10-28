import SwiftUI

/// The Export items, in the toolbar menu (`ExportFeature.itemEntries` has them).
struct ExportItems: View {
    let model: ExportModel?
    var body: some View { MenuEntriesView(entries: ExportFeature.itemEntries(model)) }
}

/// The toolbar's Export menu (`export`, "Export ▾").
struct ExportMenu: View {
    let model: ExportModel?
    var body: some View {
        Menu("Export", systemImage: "square.and.arrow.up") { ExportItems(model: model) }
            .disabled(model == nil)
            .accessibilityIdentifier("export-menu")
    }
}
