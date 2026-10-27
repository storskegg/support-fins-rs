import SwiftUI
import Viewer

/// The inspector: for M4a only the Part section, the overhang threshold, the printer volume and a temporary
/// pose. The rest of docs/UI-MAP.md arrives with M4b.
struct InspectorView: View {
    @Bindable var controller: ViewerController
    @State private var customVolume = false

    var body: some View {
        Form {
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
                Toggle("Highlight small overhangs", isOn: $controller.highlightSmall)
            }
            Section("Rotation (temporary)") {
                axis("X", \.x)
                axis("Y", \.y)
                axis("Z", \.z)
                Button("Reset") { controller.rotationDegrees = .zero }
            }
            Section("Printer") {
                Picker("Build volume", selection: $controller.volume) {
                    ForEach(PrinterVolume.presets, id: \.self) { Text($0.label).tag($0) }
                    if !controller.volume.isPreset { Text(controller.volume.label).tag(controller.volume) }
                }
            }
            #if DEBUG
            Section("Debug") {
                Toggle("Benchmark orbit", isOn: Binding(get: { controller.benchmarking }, set: { controller.setBenchmark($0) }))
            }
            #endif
        }
        .formStyle(.grouped)
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
