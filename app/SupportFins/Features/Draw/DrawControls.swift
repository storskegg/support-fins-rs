import SwiftUI

/// "+ Add walls by hand" (`augment-toggle`, docs/UI-MAP.md 4.1): in the inspector's basics in Auto and Full with fins on.
struct AugmentToggle: View {
    let model: DrawModel

    var body: some View {
        if model.augmentOffered {
            HStack {
                FindRow("augment-toggle") {
                    Toggle(model.augmentTitle, isOn: Binding(get: { model.session.addByHand }, set: { _ in model.toggleAugment() }))
                        .toggleStyle(.button)
                        .help(DrawTip.augment)
                        .accessibilityIdentifier("augment-toggle")
                }
            }
        }
    }
}

/// The Draw controls (`#draw-controls`, `index.html:432-453`): shown while hand-placed supports are, outside the
/// sections so the hint and Clear can never be collapsed away mid-draw. Line / Curve and the hint only while a click
/// places a wall; Back one point while a wall is half drawn; Remove selected while one is selected.
struct DrawControls: View {
    let model: DrawModel

    var body: some View {
        if model.controlsShown {
            FindBlock(.draw) { controls }
        }
    }

    private var controls: some View {
        VStack(alignment: .leading, spacing: 8) {
            if model.hintShown {
                // the page's Line and Curve buttons are one segmented control here: one row of the settings search
                FindRow("draw-shape") {
                    Picker("Wall shape", selection: Binding(get: { model.shape }, set: { model.setShape($0) })) {
                        Text("Line").tag(DrawModel.Shape.line)
                        Text("Curve").tag(DrawModel.Shape.curve)
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .accessibilityLabel("Wall shape")
                    .accessibilityIdentifier("draw-shape")
                }
                Text(model.hint)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("draw-hint")
            }
            HStack {
                if model.started {
                    FindRow("draw-back") {
                        Button("Back one point") { _ = model.undoPoint() }
                            .help(DrawTip.back)
                            .accessibilityIdentifier("draw-back")
                    }
                }
                if model.selectedID != nil {
                    FindRow("draw-remove") {
                        Button("Remove selected") { model.removeSelected() }
                            .help(DrawTip.removeSelected)
                            .accessibilityIdentifier("draw-remove")
                    }
                }
                FindRow("draw-clear") {
                    Button("Clear all") { model.clearAll() }
                        .disabled(!model.hasWalls)
                        .accessibilityIdentifier("draw-clear")
                }
            }
        }
    }
}

/// The web's tooltips (`index.html:423, 447-451`).
enum DrawTip {
    static let augment = JS.t("Place extra breakaway walls by hand, on top of the auto-placed ones.")
    static let back = JS.t("Take back the last point of the wall you're drawing (Backspace also works).")
    static let removeSelected = JS.t("Remove the support you clicked (Delete or Backspace also works).")
}

/// The sway brace preview's label (`#brace-tip`, `style.css:948-967`): follows the cursor, easing after it, and fades.
struct BraceTipView: View {
    let motion: BraceMotion

    var body: some View {
        let tip = motion.tip
        Text(tip.text)
            .font(.system(size: 12).monospacedDigit())
            .foregroundStyle(tip.bad ? Color(red: 1, green: 0.706, blue: 0.329) : Color.primary)
            .padding(.horizontal, 9)
            .padding(.vertical, 4)
            .frame(minWidth: 208, maxWidth: 288, alignment: .leading)
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 9))
            .overlay(RoundedRectangle(cornerRadius: 9).strokeBorder(Color.white.opacity(0.09)))
            .opacity(tip.shown ? 1 : 0)
            .animation(.easeOut(duration: 0.12), value: tip.shown)
            .offset(x: tip.at?.x ?? 0, y: tip.at?.y ?? 0)
            .allowsHitTesting(false)
            .accessibilityHidden(!tip.shown)
            .accessibilityIdentifier("brace-tip")
    }
}
