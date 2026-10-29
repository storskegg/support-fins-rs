import SwiftUI

/// The Export items of the toolbar menu (docs/UI-MAP.md `export`, `export-stl`, `export-3mf`, `export-fins`),
/// as they have always been built (the UI test finds "STL" in it). `ExportFeature.itemEntries` is the menu bar's.
struct ExportItems: View {
    let model: ExportModel?
    var board: PlateBoard?

    var body: some View {
        MenuEntriesView(entries: ExportFeature.itemEntries(model, board: board))
    }
}

/// The toolbar's Export menu (`export`, "Export ▾").
struct ExportMenu: View {
    let model: ExportModel?
    var board: PlateBoard?
    var body: some View {
        Menu("Export", systemImage: "square.and.arrow.up") { ExportItems(model: model, board: board) }
            .disabled(model == nil)
            .accessibilityIdentifier("export-menu")
    }
}
