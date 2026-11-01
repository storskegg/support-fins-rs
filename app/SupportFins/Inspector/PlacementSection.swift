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
                    modeButton(title, selected: settings.mode == mode) { settings.mode = mode }
                        .tipCard(JS.t("Who places the support."))
                        .accessibilityIdentifier("fin-mode-\(mode.rawValue)")
                        .accessibilityAddTraits(settings.mode == mode ? .isSelected : [])
                }
            }
            Text(settings.modeNote).font(.caption).foregroundStyle(.secondary)
        }
    }

    /// The chosen mode is filled with the accent colour, white text; the others plain. A tint on a `.bordered` button does
    /// not show on macOS, so the chosen mode was told apart only by a lighter bezel, and an unchosen one could read as dimmed
    /// (S3's "Auto"); `.borderedProminent` fixed that in the key window but draws as `.bordered` in one that is not key, so
    /// the fill is drawn here (S2).
    private func modeButton(_ title: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(title, action: action).buttonStyle(ModeSegmentStyle(selected: selected))
    }
}

/// One Placement button: the accent fill when chosen, a faint one otherwise, whether or not the window is key.
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
