import SwiftUI
import Viewer

/// M4a's Part, Overhang, temporary Rotation and Printer-volume sections, moved here unchanged from the old
/// `InspectorView`. The Part block becomes the stats card (Readout/) and the rotation the session's pose, so
/// this file goes when those land.
struct PartSections: View {
    @Bindable var controller: ViewerController

    var body: some View {
        Section("Part") {
            LabeledContent("File", value: controller.meshName)
            LabeledContent("Triangles", value: controller.faceCount.formatted())
            LabeledContent("Size", value: sizeText)
            if let h = controller.hoverLocation {
                LabeledContent("Face", value: controller.partCount > 1 ? "\(h.face.formatted()) (object \(h.part + 1))" : h.face.formatted())
            }
        }
        Section("Overhang") {
            LabeledContent("Threshold") {
                Slider(value: $controller.thresholdDegrees, in: 30...70, step: 1) { Text("Threshold") }
                Text("\(Int(controller.thresholdDegrees))°").monospacedDigit().frame(width: 36, alignment: .trailing)
            }
        }
        Section("Rotation (temporary)") {
            axis("X", \.x)
            axis("Y", \.y)
            axis("Z", \.z)
            Button("Reset") { controller.rotationDegrees = .zero }
        }
        Section("Printer volume") {
            Picker("Build volume", selection: $controller.volume) {
                ForEach(PrinterVolume.presets, id: \.self) { Text($0.label).tag($0) }
                if !controller.volume.isPreset { Text(controller.volume.label).tag(controller.volume) }
            }
        }
    }

    private var sizeText: String {
        let s = controller.size
        return String(format: "%.1f × %.1f × %.1f mm", s.x, s.y, s.z)
    }

    private func axis(_ name: String, _ key: WritableKeyPath<SIMD3<Float>, Float>) -> some View {
        LabeledContent(name) {
            Slider(value: Binding(get: { Double(controller.rotationDegrees[keyPath: key]) },
                                  set: { controller.rotationDegrees[keyPath: key] = Float($0) }), in: -180...180, step: 1)
            Text("\(Int(controller.rotationDegrees[keyPath: key]))°").monospacedDigit().frame(width: 40, alignment: .trailing)
        }
    }
}
