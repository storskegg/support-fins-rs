import SwiftUI

/// The Strength popover (`#strength-pop`, `ui/strength.js`): the six-way load pad, Clear, the verdict in the
/// page's good / ok / weak styles, and "Turn to the strongest printable pose".
struct StrengthPopover: View {
    @Bindable var model: StrengthModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("\(Text("Strength arrow").bold()): \(Text("which way is it loaded?"))")
            Grid(horizontalSpacing: 4, verticalSpacing: 4) {
                GridRow {
                    pad(.up); pad(.down); pad(.left)
                }
                GridRow {
                    pad(.right); pad(.front); pad(.back)
                }
            }
            if model.isLit {
                Button("Clear") { model.clear() }.accessibilityIdentifier("load-clear")
            }
            if let note = model.note {
                Text(note)
                    .font(.callout)
                    .foregroundStyle(Self.color(model.quality))
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("load-note")
            }
            if model.canSuggest {
                Button(model.thinking ? "Turning…" : "Turn to the strongest printable pose") { model.turnToStrongest() }
                    .disabled(model.thinking)
                    .accessibilityIdentifier("load-suggest")
            }
            if let error = model.error { Text(error).font(.callout).foregroundStyle(.red) }
        }
        .frame(width: 300, alignment: .leading)
    }

    private func pad(_ p: StrengthModel.Pad) -> some View {
        Button(p.label) { model.setLoad(p) }
            .help(p.help)
            .buttonStyle(.bordered)
            .tint(model.activePad == p ? .accentColor : nil)
            .accessibilityLabel(p.help)
            .accessibilityIdentifier("load-\(p.rawValue)")
    }

    /// good / ok / weak (`.load-verdict.good|mixed|poor`).
    static func color(_ quality: String?) -> Color {
        switch quality {
        case "good": .green
        case "mixed": .orange
        case "poor": .red
        default: .secondary
        }
    }
}
