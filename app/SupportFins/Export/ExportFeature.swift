import SwiftUI

/// Export: the window toolbar's Export menu, and File > Export with ⌘E for the last format used.
@MainActor
final class ExportFeature: Feature {
    unowned let host: ViewerModel

    init(host: ViewerModel) { self.host = host }

    var windowToolbarItems: [WindowToolbarItem] {
        [WindowToolbarItem("export", order: 0) { ExportMenu(model: self.host.wiring?.exporter, board: self.host.features[PlatesFeature.self]?.board) }]
    }

    /// The Export items (docs/UI-MAP.md `export`, `export-stl`, `export-3mf`, `export-fins`, `export-all`,
    /// `export-arrange`): disabled until there is something to write, with the web's reason (`#export-nofins`) under
    /// them. With several models the 3MF is the plate's, with several plates there is "All plates (3MF)", and models
    /// that overlap say so first, with "Arrange first" (`setOpen`, `ui/export.js`).
    static func itemEntries(_ model: ExportModel?, board: PlateBoard? = nil) -> [MenuEntry] {
        let available = model?.availability
        let multi = (board?.modelCount ?? 0) > 1
        let ready = available.map { !$0.enabled.isEmpty } ?? false
        var entries: [MenuEntry] = []
        // models that collide print into each other: say so where the file is written (the plate's 3MF; an STL is the
        // model being edited alone), Arrange to hand
        if ready, board?.clashing == true {
            entries += [.note(JS.t("Models overlap: the 3MF prints them into each other.")),
                        .item(JS.t("Arrange first")) { Task { await board?.arrange() } }]
        }
        entries += ExportFormat.allCases.map { format in
            if format == .threeMF, multi, let board {
                return .item(format.label, help: format.help, enabled: ready && board.canExportPlate) {
                    Task { await board.exportPlate() }
                }
            }
            return .item(format.label, help: format.help, enabled: available?.enabled.contains(format) ?? false) { model?.run(format) }
        }
        if (board?.plates.count ?? 0) > 1, let board {
            entries.append(.item(JS.t("All plates (3MF)"), help: JS.t("Every plate's part and supports, one 3MF each, in one .zip file."),
                                 enabled: !board.busy) { Task { await board.exportAll() } })
        }
        if let note = available?.note, !(multi && board?.parkedFinsExist == true) { entries += [.divider, .note(note)] }
        return entries
    }

    /// Write `format` again (⌘E): the plate's 3MF when there are several models.
    static func run(_ format: ExportFormat, model: ExportModel?, board: PlateBoard?) {
        if format == .threeMF, (board?.modelCount ?? 0) > 1, let board {
            Task { await board.exportPlate() }
        } else {
            _ = model?.run(format)
        }
    }

    /// docs/UI-MAP.md "Export > STL / 3MF / Fins only (⌘E for the last used)".
    static func menus(_ f: ExportFeature?) -> [MenuSection] {
        let exporter = f?.host.wiring?.exporter
        let board = f?.host.features[PlatesFeature.self]?.board
        let again = exporter?.lastFormat ?? .stl
        return [MenuSection(menu: .fileExport, order: 0, entries: [
            .submenu(JS.t("Export"), itemEntries(exporter, board: board)),
            .item(JS.t("Export Again"), shortcut: "e", enabled: exporter?.availability.enabled.contains(again) ?? false) {
                run(again, model: exporter, board: board)
            },
        ])]
    }
}
