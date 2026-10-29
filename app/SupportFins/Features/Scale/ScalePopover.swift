import SwiftUI

/// The Scale popover (`#scale-pop`, `index.html:658-663`): the percent field, Reset and the hint. A new scale is a
/// whole reload, so the field takes a typed value on Return or when it loses focus only, with no steps and the
/// Up / Down keys swallowed (`ui/scale.js:77-83`).
struct ScalePopover: View {
    @Bindable var model: ScaleModel
    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Text("Scale")
                TextField("Scale", text: $text)
                    .labelsHidden()
                    .multilineTextAlignment(.trailing)
                    .frame(width: 70)
                    .focused($focused)
                    .onSubmit { text = model.commit(text: text) }
                    .onKeyPress(.upArrow) { .handled }
                    .onKeyPress(.downArrow) { .handled }
                    .accessibilityIdentifier("scale-pct")
                Text("%")
                Button("Reset") { model.reset() }
                    .disabled(!model.isScaled)
                    .accessibilityIdentifier("scale-reset")
            }
            Text("A new scale clears drawn walls, removed fins and undo.")
                .font(.callout).foregroundStyle(.secondary)
        }
        .disabled(!model.hasPart)
        .onAppear { text = model.fieldText }
        .onChange(of: model.scale) { _, _ in text = model.fieldText }
        .onChange(of: focused) { was, now in if was && !now { text = model.commit(text: text) } }
    }
}
