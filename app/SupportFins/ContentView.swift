import SwiftUI
import Viewer

struct ContentView: View {
    let document: PartDocument
    @Environment(\.documentConfiguration) private var configuration
    @State private var model = ViewerModel()
    @State private var inspector = true
    @State private var settings = SettingsStore().load()

    var body: some View {
        ZStack(alignment: .topLeading) {
            if let controller = model.controller {
                ViewerViewport(controller: controller)
                    .ignoresSafeArea()
                    #if DEBUG
                    .overlay(alignment: .topLeading) { FrameOverlay(controller: controller) }
                    #endif
            }
            if let status = model.status {
                Text(status).padding(8).background(.regularMaterial, in: .rect(cornerRadius: 8)).padding()
            }
        }
        .inspector(isPresented: $inspector) {
            if let controller = model.controller {
                InspectorView(controller: controller, settings: $settings)
                    .inspectorColumnWidth(min: 240, ideal: 280, max: 360)
            }
        }
        .toolbar {
            ToolbarItem { Toggle("Inspector", systemImage: "sidebar.right", isOn: $inspector) }
        }
        .navigationTitle(document.filename)
        .task {
            await model.load(url: configuration?.fileURL)
            await Benchmark.runIfRequested(model.controller)
        }
        .focusedSceneValue(\.viewer, model.controller)
    }
}
