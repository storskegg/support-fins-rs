import SwiftUI

/// The Section popover (`#section-pop`, `index.html:705-715`): Ghost / Hide, Flip and Re-aim. Every tooltip is
/// the web's.
struct SectionPopover: View {
    @Bindable var model: SectionModel

    var body: some View {
        HStack(spacing: 8) {
            // "What the cut does to the near half"
            HStack(spacing: 0) {
                choice("Ghost", on: model.ghost, id: "section-ghost",
                       help: "Show the cut-away half as faint glass. Clicks go through it.") { model.setGhost(true) }
                choice("Hide", on: !model.ghost, id: "section-hide",
                       help: "Leave the cut-away half out entirely.") { model.setGhost(false) }
            }
            .accessibilityElement(children: .contain)
            .accessibilityLabel("What the cut does to the near half")
            Button("Flip") { model.flip() }
                .help("Show the other side of the cut instead.")
                .accessibilityIdentifier("section-flip")
            Button("Re-aim") { model.reaim() }
                .help("Aim the cut at the current view again, through the part's centre.")
                .accessibilityIdentifier("section-reset")
        }
        .controlSize(.small)
    }

    @ViewBuilder
    private func choice(_ title: LocalizedStringKey, on: Bool, id: String, help: LocalizedStringKey,
                        action: @escaping () -> Void) -> some View {
        Button(action: action) { Text(title) }
            .buttonStyle(SegmentStyle(selected: on))
            .help(help)
            .accessibilityIdentifier(id)
            .accessibilityAddTraits(on ? .isSelected : [])
    }
}

/// One half of the Ghost / Hide segmented pair (`.btn.sm.primary` when it is the one in force).
private struct SegmentStyle: ButtonStyle {
    let selected: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .padding(.horizontal, 10).padding(.vertical, 3)
            .foregroundStyle(selected ? AnyShapeStyle(.white) : AnyShapeStyle(.primary))
            .background(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.quaternary.opacity(configuration.isPressed ? 1 : 0.5)),
                        in: .rect(cornerRadius: 5))
    }
}
