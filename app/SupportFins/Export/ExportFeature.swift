import SwiftUI

/// Export: the window toolbar's Export menu, and File > Export with ⌘E for the last format used.
@MainActor
final class ExportFeature: Feature {
    unowned let host: ViewerModel

    init(host: ViewerModel) { self.host = host }

    var windowToolbarItems: [WindowToolbarItem] {
        [WindowToolbarItem("export", order: 0) { ExportMenu(model: self.host.wiring?.exporter) }]
    }

    /// The Export items (docs/UI-MAP.md `export`, `export-stl`, `export-3mf`, `export-fins`): disabled until there is
    /// something to write, with the web's reason (`#export-nofins`) under them.
    static func itemEntries(_ model: ExportModel?) -> [MenuEntry] {
        let available = model?.availability
        var entries: [MenuEntry] = ExportFormat.allCases.map { format in
            .item(format.label, help: format.help, enabled: available?.enabled.contains(format) ?? false) { model?.run(format) }
        }
        if let note = available?.note { entries += [.divider, .note(note)] }
        return entries
    }

    /// docs/UI-MAP.md "Export > STL / 3MF / Fins only (⌘E for the last used)".
    static func menus(_ f: ExportFeature?) -> [MenuSection] {
        let exporter = f?.host.wiring?.exporter
        return [MenuSection(menu: .fileExport, order: 0, entries: [
            .submenu("Export", itemEntries(exporter)),
            .item("Export Again", shortcut: "e",
                  enabled: exporter?.availability.enabled.contains(exporter?.lastFormat ?? .stl) ?? false) {
                exporter.map { _ = $0.run($0.lastFormat) }
            },
        ])]
    }
}
