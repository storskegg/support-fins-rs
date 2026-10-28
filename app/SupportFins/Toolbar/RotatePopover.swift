import SwiftUI

/// The Rotate popover (`#rotate-pop`, `index.html:637-651`, `ui/pose.js`): Rotate 90° X / Y / Z, Reset, the
/// live readout and the delta, and Show rotate rings. Lay a face flat sits between the readout and the rings switch (`index.html:646-651`).
struct RotatePopover: View {
    @Bindable var model: RotateModel
    @Bindable var layFace: LayFaceModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if model.showRings { Text("Drag the rings to turn it, or:").font(.callout).foregroundStyle(.secondary) }
            HStack {
                Text("Rotate 90°")
                ForEach(Pose.Axis.allCases, id: \.self) { axis in
                    Button(axis.label) { model.rotate90(about: axis) }
                        .help("Rotate 90 degrees about \(axis.label)")
                        .accessibilityLabel("Rotate 90 degrees about \(axis.label)")
                        .accessibilityIdentifier("rot-\(axis.label.lowercased())")
                }
                Button("Reset") { model.reset() }.accessibilityIdentifier("rot-reset")
            }
            .disabled(!model.hasPart)
            HStack(spacing: 6) {
                Text(model.now).accessibilityIdentifier("rot-now")
                Text(model.delta).foregroundStyle(.secondary).accessibilityIdentifier("rot-delta")
            }
            .font(.callout.monospacedDigit())
            LayFaceButton(model: layFace)
            Toggle("Show rotate rings", isOn: $model.showRings)
                .help("The rotate rings around the part. Off hides them; Rotate 90°, Reset and Lay a face flat still turn the part.")
                .accessibilityIdentifier("show-rings")
        }
        .frame(width: 300, alignment: .leading)
    }
}
