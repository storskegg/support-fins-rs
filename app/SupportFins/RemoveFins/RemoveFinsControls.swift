import SwiftUI

/// `remove-fins-toggle` and `restore-fins` (docs/UI-MAP.md): in the inspector's basics, shown only in Auto
/// and Full with fins on screen.
struct RemoveFinsControls: View {
    let model: RemovalModel?

    var body: some View {
        if let model, model.controlsShown {
            HStack {
                FindRow("remove-fins-toggle") {
                    Toggle(model.toggleLabel, isOn: Binding(get: { model.armed }, set: { _ in model.toggle() }))
                        .toggleStyle(.button)
                        .help("Click a fin to remove just that one. Esc or right-click cancels.")
                }
                if model.canRestore {
                    FindRow("restore-fins") {
                        Button("Restore all") { model.restoreAll() }
                            .help("Bring back every fin removed in this orientation.")
                    }
                }
            }
        }
    }
}
