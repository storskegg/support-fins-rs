import SwiftUI

struct RemovalKey: FocusedValueKey { typealias Value = RemovalModel }
extension FocusedValues {
    var removal: RemovalModel? {
        get { self[RemovalKey.self] }
        set { self[RemovalKey.self] = newValue }
    }
}

/// `remove-fins-toggle` and `restore-fins` (docs/UI-MAP.md): in the inspector's basics, shown only in Auto
/// and Full with fins on screen.
struct RemoveFinsControls: View {
    @FocusedValue(\.removal) private var model

    var body: some View {
        if let model, model.controlsShown {
            HStack {
                Toggle(model.toggleLabel, isOn: Binding(get: { model.armed }, set: { _ in model.toggle() }))
                    .toggleStyle(.button)
                    .help("Click a fin to remove just that one. Esc or right-click cancels.")
                if model.canRestore {
                    Button("Restore all") { model.restoreAll() }
                        .help("Bring back every fin removed in this orientation.")
                }
            }
        }
    }
}
