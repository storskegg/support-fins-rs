import SwiftUI

/// Calibration prints (`ui/calibrate.js`): Help > Calibration Prints… opens the sheet of test prints, Help > Full
/// Calibration Guide the guide (docs/CALIBRATION.md) in the browser.
@MainActor
final class CalibrationFeature: Feature {
    unowned let host: ViewerModel
    let model = CalibrationModel()

    init(host: ViewerModel) { self.host = host }

    var modifiers: [WindowModifier] {
        [WindowModifier("calibration-sheet") { content in AnyView(CalibrationHost(model: self.model, content: content)) }]
    }

    static func menus(_ f: CalibrationFeature?) -> [MenuSection] {
        [MenuSection(menu: .help, order: 10, entries: [
            .item("Calibration Prints…", help: "Test prints that tune the settings to your printer and filament.",
                  enabled: f != nil) { f?.model.show() },
            .item("Full Calibration Guide", help: "How to read each test print: docs/CALIBRATION.md.") {
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
