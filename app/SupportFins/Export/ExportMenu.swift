import SwiftUI

/// The Export items of the toolbar menu (docs/UI-MAP.md `export`, `export-stl`, `export-3mf`, `export-fins`),
/// as they have always been built (the UI test finds "STL" in it). `ExportFeature.itemEntries` is the menu bar's.
struct ExportItems: View {
    let model: ExportModel?

    var body: some View {
        let available = model?.availability
        ForEach(ExportFormat.allCases, id: \.self) { format in
            Button(format.label) { model?.run(format) }
                .disabled(!(available?.enabled.contains(format) ?? false))
                .help(format.help)
        }
        if let note = available?.note {
            Divider()
            Text(note)
        }
    }
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
