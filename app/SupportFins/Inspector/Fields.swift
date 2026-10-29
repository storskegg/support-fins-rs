import SwiftUI

/// A number field with a stepper, clamped to its limits on every edit (the web clamps in `wireGap` and
/// `readPadCustom`, `web/ui/settings.js`). `unit` follows the value as a muted suffix.
struct NumberField: View {
    let title: LocalizedStringKey
    @Binding var value: Double
    let limits: FinSettings.Limits
    var unit: LocalizedStringKey?
    var changed = false
    var help = ""

    var body: some View {
        LabeledContent {
            HStack(spacing: 4) {
                TextField(title, value: clamped, format: .number.precision(.fractionLength(0...3)))
                    .labelsHidden()
                    .multilineTextAlignment(.trailing)
                    .frame(width: 64)
                if let unit { Text(unit).foregroundStyle(.secondary) }
                Stepper(title, value: clamped, in: limits.range, step: limits.step).labelsHidden()
            }
        } label: {
            ChangedLabel(title, changed: changed)
        }
        .help(help)
    }

    private var clamped: Binding<Double> {
        Binding(get: { value }, set: { value = min(limits.range.upperBound, max(limits.range.lowerBound, $0)) })
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
            if let detail { Text(title) + Text(" ") + Text(detail).foregroundStyle(.secondary) } else { Text(title) }
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

    var body: some View {
        Toggle(isOn: $isOn) { ChangedLabel(title, detail: detail, changed: changed) }.help(help)
    }
}
