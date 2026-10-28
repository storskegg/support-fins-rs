import SwiftUI
import Viewer

/// The View popover (`#view-pop`, `index.html:699-710`): the camera views, the Mouse preset with its hint and
/// Show layers. The views are the View menu's (⌘1..⌘6): both call `ViewerController.apply`.
struct ViewPopover: View {
    let controller: ViewerController
    @Bindable var settings: ViewSettings

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            // opposite sides paired in columns: Front/Back, Left/Right, Top/Bottom (style.css `#cam-views`)
            Grid(horizontalSpacing: 4, verticalSpacing: 4) {
                GridRow { button(.front); button(.left); button(.top) }
                GridRow { button(.back); button(.right); button(.bottom) }
            }
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Camera view")
            VStack(alignment: .leading, spacing: 4) {
                Picker("Mouse", selection: $settings.navPreset) {
                    ForEach(NavPreset.allCases) { Text($0.label).tag($0) }
                }
                .help(settings.navPreset.hint)
                .accessibilityIdentifier("nav-preset")
                Text(settings.navPreset.hint)
                    .font(.callout).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("nav-hint")
            }
            Toggle("Show layers", isOn: Binding(get: { controller.showsLayers }, set: { controller.showsLayers = $0 }))
                .help("Show the horizontal print layers around the part.")
                .accessibilityIdentifier("show-layers")
            Toggle("Rebuild while dragging", isOn: $settings.rebuildWhileDragging)
                .help("Rebuild the fins while a rotate ring is dragged, so they follow the part. Off builds them when the drag ends.")
                .accessibilityIdentifier("rebuild-while-dragging")
        }
        .frame(width: 250, alignment: .leading)
    }

    private func button(_ view: CameraView) -> some View {
        Button(view.label) { controller.apply(view) }
            .buttonStyle(.bordered)
            .frame(maxWidth: .infinity)
            .help(view.hint)
            .accessibilityIdentifier("view-\(view.rawValue)")
    }
}
