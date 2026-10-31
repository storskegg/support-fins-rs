import SwiftUI

/// A number field with a stepper, clamped to its limits on every edit (the web clamps in `wireGap` and
/// `readPadCustom`, `web/ui/settings.js`). `unit` follows the value as a muted suffix.
///
/// The field edits its own text, as the web's `<input>` does: each key parses it and writes the value held to the
/// limits, and the text stays as typed until the edit ends (Return, or focus moving on), when it shows the value. A
/// `TextField(value:format:)` cannot do that: it wrote the held value back over the text at the next redraw, so 0.1
/// typed over 0.05 read "0.01.1" (the first "0" held to the 0.01 minimum; `NumberFieldTests`).
/// The text never takes `value` back while it parses: `value` may be a binding over a snapshot of the settings, a key
/// behind (`endEdit`).
struct NumberField: View {
    let title: LocalizedStringKey
    @Binding var value: Double
    let limits: FinSettings.Limits
    var unit: LocalizedStringKey?
    var changed = false
    var help = ""
    /// the key of the setting's tip-card picture (`data-tip-pic`)
    var pic: String?
    /// the field's accessibility identifier (UI tests)
    var fieldID: String?

    /// The field's text: what was typed while an edit is on, the value formatted otherwise.
    @State private var text = ""
    @FocusState private var editing: Bool
    @Environment(\.locale) private var locale

    var body: some View {
        LabeledContent {
            HStack(spacing: 4) {
                TextField(title, text: $text)
                    .focused($editing)
                    .onAppear { text = shown(value) }
                    .onChange(of: text) { _, typed in
                        // a key: the value follows what parses, held to the limits; the text is left as typed
                        guard let v = parsed(typed) else { return }
                        let held = Self.clamp(v, to: limits)
                        if held != value { value = held }
                    }
                    .onChange(of: value) { _, v in
                        // from outside (a printer picked, the stepper, undo): shown, unless it is what the text says
                        if parsed(text).map({ Self.clamp($0, to: limits) }) != v { text = shown(v) }
                    }
                    .onSubmit(endEdit)
                    .onChange(of: editing) { _, on in if !on { endEdit() } }
                    .labelsHidden()
                    .multilineTextAlignment(.trailing)
                    .frame(width: 64)
                    .accessibilityIdentifier(fieldID ?? "")
                if let unit { Text(unit).foregroundStyle(.secondary) }
                Stepper(title, value: clamped, in: limits.range, step: limits.step).labelsHidden()
            }
        } label: {
            ChangedLabel(title, changed: changed)
        }
        .tipCard(pic: pic, help)
    }

    private var clamped: Binding<Double> {
        Binding(get: { value }, set: { value = Self.clamp($0, to: limits) })
    }

    /// The edit ends: the text shows the value it set, held to the limits (the value when it does not parse). Read from
    /// the text, not from `value`: a binding over a snapshot (the Plate ties section's `number(s, …)`) still reads what
    /// the section drew before the last key, and showing that wrote it back (15 typed into Max gap went back to 1).
    private func endEdit() {
        let t = shown(parsed(text).map { Self.clamp($0, to: limits) } ?? value)
        if t != text { text = t }
    }

    private func shown(_ v: Double) -> String { Self.format.locale(locale).format(v) }
    private func parsed(_ s: String) -> Double? { try? Self.format.locale(locale).parseStrategy.parse(s) }

    /// How the field shows and reads a number: up to three decimals.
    static let format = FloatingPointFormatStyle<Double>.number.precision(.fractionLength(0...3))

    /// A value as the field keeps it: held to the limits, never snapped to the step (the Stepper steps from it).
    static func clamp(_ v: Double, to limits: FinSettings.Limits) -> Double {
        min(limits.range.upperBound, max(limits.range.lowerBound, v))
    }
}

/// A field's name with the web's dot (`.changed`) when its value is not the default (`web/ui/more.js:44`).
struct ChangedLabel: View {
    let title: LocalizedStringKey
    /// The muted second half of a label (`<span class="mut">`: "Long walls" "along the part")
    let detail: LocalizedStringKey?
    let changed: Bool
    init(_ title: LocalizedStringKey, detail: LocalizedStringKey? = nil, changed: Bool) {
        self.title = title; self.detail = detail; self.changed = changed
    }

    var body: some View {
        HStack(spacing: 5) {
            if let detail { Text("\(Text(title)) \(Text(detail).foregroundStyle(.secondary))") } else { Text(title) }
            if changed {
                Circle().fill(Color.accentColor).frame(width: 6, height: 6)
                    .accessibilityLabel("changed")
            }
        }
    }
}

/// A toggle with the changed dot.
struct ChangedToggle: View {
    let title: LocalizedStringKey
    var detail: LocalizedStringKey?
    @Binding var isOn: Bool
    var changed = false
    var help = ""
    var pic: String?

    var body: some View {
        Toggle(isOn: $isOn) { ChangedLabel(title, detail: detail, changed: changed) }.tipCard(pic: pic, help)
    }
}
