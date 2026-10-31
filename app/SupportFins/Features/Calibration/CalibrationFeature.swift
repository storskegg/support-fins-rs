import SwiftUI

/// Calibration prints (`ui/calibrate.js`): Help > Calibration Prints… opens the sheet of test prints, Help > Full
/// Calibration Guide the guide (docs/CALIBRATION.md) in the browser.
@MainActor
final class CalibrationFeature: Feature {
    weak let host: ViewerModel?
    let model = CalibrationModel()

    init(host: ViewerModel) {
        self.host = host
        model.tieInputs = { [weak host] in host.flatMap(Self.tieInputs) }
    }

    /// The Plate ties print's inputs (issue #50): the printer picked, this window's fin settings and its bed.
    static func tieInputs(_ host: ViewerModel) -> TiePrintInputs? {
        let store = PrinterProfileStore.shared
        guard let printer = store.current else { return nil }
        let s = host.session.settings
        let bed = host.controller.map { [Double($0.volume.x), Double($0.volume.y)] } ?? []
        return TiePrintInputs(profile: printer.id, printer: printer.name, options: s.optionsJSON(), tunables: s.tunablesJSON(), bed: bed)
    }

    var modifiers: [WindowModifier] {
        [WindowModifier("calibration-sheet") { content in AnyView(CalibrationHost(model: self.model, content: content)) }]
    }

    static func menus(_ f: CalibrationFeature?) -> [MenuSection] {
        [MenuSection(menu: .help, order: 10, entries: [
            .item(JS.t("Calibration Prints…"), help: JS.t("Test prints that tune the settings to your printer and filament."),
                  enabled: f != nil) { f?.model.show() },
            .item(JS.t("Full Calibration Guide")) {
                NSWorkspace.shared.open(CalibrationCatalog.guide)
            },
        ])]
    }
}

private struct CalibrationHost: View {
    @Bindable var model: CalibrationModel
    let content: AnyView

    var body: some View {
        content.sheet(isPresented: $model.isShowing) { CalibrationSheet(model: model) }
    }
}
