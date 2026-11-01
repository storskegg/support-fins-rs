import SwiftUI

/// Placement: Auto / Full coverage / Draw, with the per-mode note (`index.html:329-355`,
/// `web/ui/settings.js:79-115`). Draw is the by-hand mode (Features/Draw).
struct PlacementSection: View {
    @Binding var settings: FinSettings

    private static let modes: [(FinSettings.Mode, String)] = [(.auto, JS.t("Auto")), (.full, JS.t("Full coverage")), (.draw, JS.t("Draw"))]

    var body: some View {
        FindRow("fin-mode-fld") { picker }
    }

    private var picker: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Placement")
            HStack(spacing: 2) {
                ForEach(Self.modes, id: \.0) { mode, title in
                    Button(title) { settings.mode = mode }
                        .buttonStyle(ModeSegmentStyle(selected: settings.mode == mode))
                        .tipCard(JS.t("Who places the support."))
                        .accessibilityIdentifier("fin-mode-\(mode.rawValue)")
                        .accessibilityAddTraits(settings.mode == mode ? .isSelected : [])
                }
            }
            Text(settings.modeNote).font(.caption).foregroundStyle(.secondary)
        }
    }
}

/// One of the Placement buttons: the chosen mode filled with the accent colour, the others plain, whether or not the
/// window is the key one. (`.bordered` tinted with the accent drew the chosen mode paler than the others on macOS 26,
/// so Auto, chosen, looked switched off: S2, November 2026.)
struct ModeSegmentStyle: ButtonStyle {
    var selected: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .padding(.horizontal, 10).padding(.vertical, 4)
            .foregroundStyle(selected ? Color.white : Color.primary)
            .background(RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(selected ? Color.accentColor : Color.primary.opacity(0.08)))
            .opacity(configuration.isPressed ? 0.75 : 1)
            .contentShape(Rectangle())
    }
}
