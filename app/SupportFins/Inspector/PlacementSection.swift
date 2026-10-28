import SwiftUI

/// Placement: Auto / Full coverage / Draw, with the per-mode note (`index.html:329-355`,
/// `web/ui/settings.js:79-115`). Draw is drawn by hand: shown, disabled until M6.
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
                        .disabled(mode == .draw)
                        .help(mode == .draw ? "Draw arrives with the pointer tools (M6)." : "Who places the support.")
                        .accessibilityAddTraits(settings.mode == mode ? .isSelected : [])
                }
            }
            Text(settings.modeNote).font(.caption).foregroundStyle(.secondary)
        }
    }
}
