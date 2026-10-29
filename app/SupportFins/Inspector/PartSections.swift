import SwiftUI
import Viewer

/// M4a's Part block, kept for a window with no readout card. The Overhang slider and the Build volume pop-up
/// are in the window toolbar (`Toolbar/TopBarItems.swift`), where the web has them (UI-MAP section 2).
struct PartSections: View {
    @Bindable var controller: ViewerController
    /// The M4a Part block; false when the stats card stands in for it.
    var withPartBlock = true

    var body: some View {
        if withPartBlock { partBlock }
    }

    private var partBlock: some View {
        Section("Part") {
            LabeledContent("File", value: controller.meshName)
            LabeledContent("Triangles", value: controller.faceCount.formatted())
            LabeledContent("Size", value: sizeText)
            if let h = controller.hoverLocation {
                LabeledContent("Face", value: controller.partCount > 1 ? JS.t("{face} (object {object})", ["face": h.face.formatted(), "object": String(h.part + 1)]) : h.face.formatted())
            }
        }
    }

    private var sizeText: String {
        let s = controller.size
        return String(format: "%.1f × %.1f × %.1f mm", s.x, s.y, s.z)
    }
}
