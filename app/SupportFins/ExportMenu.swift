import SwiftUI

struct ExporterKey: FocusedValueKey { typealias Value = ExportModel }
extension FocusedValues {
    var exporter: ExportModel? {
        get { self[ExporterKey.self] }
        set { self[ExporterKey.self] = newValue }
    }
}

/// The Export items, in the toolbar menu and in File > Export (docs/UI-MAP.md `export`, `export-stl`,
/// `export-3mf`, `export-fins`): disabled until there is something to write, with the web's reason
/// (`#export-nofins`) under them.
struct ExportItems: View {
    let model: ExportModel?

    var body: some View {
        let available = model?.availability
        ForEach(ExportFormat.allCases, id: \.self) { format in
            Button(format.label) { model?.run(format) }
                .disabled(!(available?.enabled.contains(format) ?? false))
                .help(format.help)
        }
        if let note = available?.note ?? model?.multiObjectNote {
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

/// File > Export, and ⌘E for the last format used (docs/UI-MAP.md "Export > STL / 3MF / Fins only (⌘E for the
/// last used)").
struct ExportCommands: Commands {
    @FocusedValue(\.exporter) private var exporter

    var body: some Commands {
        CommandGroup(after: .newItem) {
            Menu("Export") { ExportItems(model: exporter) }
            Button("Export Again") { exporter.map { _ = $0.run($0.lastFormat) } }
                .keyboardShortcut("e")
                .disabled(!(exporter?.availability.enabled.contains(exporter?.lastFormat ?? .stl) ?? false))
        }
    }
}
