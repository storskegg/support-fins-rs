import SwiftUI
import Viewer

/// M4a's Part, Overhang and Printer-volume sections, moved here from the old `InspectorView`. The Part block
/// becomes the stats card (Readout/) when that lands, and this file goes with it. (The temporary rotation is
/// gone: the pose is the session's, turned by the rings and the Rotate commands.)
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
}
