import SwiftUI

/// Placement: Auto / Full coverage / Draw, with the per-mode note (`index.html:329-355`,
/// `web/ui/settings.js:79-115`). Draw is the by-hand mode (Features/Draw).
struct PlacementSection: View {
    @Binding var settings: FinSettings

    private static let modes: [(FinSettings.Mode, String)] = [(.auto, "Auto"), (.full, "Full coverage"), (.draw, "Draw")]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Placement")
            HStack(spacing: 0) {
                ForEach(Self.modes, id: \.0) { mode, title in
                    Button(title) { settings.mode = mode }
                        .buttonStyle(.bordered)
                        .tint(settings.mode == mode ? .accentColor : nil)
                        .help("Who places the support.")
                        .accessibilityIdentifier("fin-mode-\(mode.rawValue)")
                        .accessibilityAddTraits(settings.mode == mode ? .isSelected : [])
                }
            }
            Text(settings.modeNote).font(.caption).foregroundStyle(.secondary)
        }
    }
}
