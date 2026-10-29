import SwiftUI
import Viewer

/// The window's frame: the viewport, the inspector, the window toolbar's fixed items and the title. Everything a
/// feature adds comes from `model.features` (Features/README.md), composed here in a fixed order.
struct ContentView: View {
    let document: PartDocument
    @Environment(\.documentConfiguration) private var configuration
    @State private var model = ViewerModel()
    @State private var inspector = true
    @Environment(\.undoManager) private var undoManager

    var body: some View {
        let features = model.features!
        featureLayers(features)
            .focusedSceneValue(\.features, features)
            // Esc ends the armed mode wherever the focus is (the viewer handles it too when it has the keys)
            .onExitCommand { features.escape() }
            .environment(model.session)
    }

    private func featureLayers(_ features: FeatureSet) -> some View {
        let overlays = features.overlays
        return features.modifiers.reduce(AnyView(main(features, overlays))) { $1.apply($0) }
    }

    private func main(_ features: FeatureSet, _ overlays: [WindowOverlay]) -> some View {
        ZStack(alignment: .topLeading) {
            Color.clear
            if let controller = model.controller {
                ViewerViewport(controller: controller)
                    .ignoresSafeArea()      // its accessibility identifier, `viewport`, is the view's own (ViewerViewport)
            }
        }
        .overlay { OverlayStack(overlays: overlays) }
        .environment(model.toolbar)
        .inspector(isPresented: $inspector) {
            if let controller = model.controller {
                InspectorView(controller: controller, settings: model.session.settingsBinding,
                              top: features.inspectorSections(.top), basics: features.inspectorSections(.basics),
                              bottom: features.inspectorSections(.bottom))
                    .inspectorColumnWidth(min: 240, ideal: 280, max: 360)
            }
        }
        .toolbar {
            ToolbarItem { if let controller = model.controller { VolumePicker(controller: controller) } }
            ToolbarItem { OverhangSlider(session: model.session) }
            ToolbarItem { FinsToggle(session: model.session) }
            ForEach(features.windowToolbarItems) { item in ToolbarItem { ContributionView(content: item.content) } }
            ToolbarItem { Toggle("Inspector", systemImage: "sidebar.right", isOn: $inspector) }
        }
        .navigationTitle(model.openedName ?? document.filename)
        .task {
            await model.load(url: configuration?.fileURL)
            await Benchmark.runIfRequested(model.controller)
        }
        // the overhang threshold is a session setting; the viewer re-analyses at it (`web/ui/part.js:503`)
        .onChange(of: model.session.settings.threshold, initial: true) { _, t in model.controller?.thresholdDegrees = t }
        .onChange(of: model.session.pose) { _, pose in model.controller?.rotation = pose.float3x3 }
        .onChange(of: undoManager, initial: true) { _, m in model.session.undoManager = m }
    }
}

/// The features' layers over the viewport, in `order`: each fills the window and aligns its content.
private struct OverlayStack: View {
    var overlays: [WindowOverlay]

    var body: some View {
        ZStack {
            ForEach(overlays) { layer in
                if layer.ignoresSafeArea {
                    layer.content().frame(maxWidth: .infinity, maxHeight: .infinity, alignment: layer.alignment).ignoresSafeArea()
                } else {
                    layer.content().frame(maxWidth: .infinity, maxHeight: .infinity, alignment: layer.alignment)
                }
            }
        }
    }
}
