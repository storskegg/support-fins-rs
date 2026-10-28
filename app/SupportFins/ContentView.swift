import SwiftUI
import Viewer

struct ContentView: View {
    let document: PartDocument
    @Environment(\.documentConfiguration) private var configuration
    @State private var model = ViewerModel()
    @State private var inspector = true
    @Environment(\.undoManager) private var undoManager

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
            if let w = model.wiring, w.spinnerVisible { BuildSpinner() }
        }
        .inspector(isPresented: $inspector) {
            if let controller = model.controller {
                InspectorView(controller: controller, settings: model.session.settingsBinding)
                    .inspectorColumnWidth(min: 240, ideal: 280, max: 360)
            }
        }
        .toolbar {
            ToolbarItem { FinsToggle(session: model.session) }
            ToolbarItem { ExportMenu(model: model.wiring?.exporter) }
            ToolbarItem { Toggle("Inspector", systemImage: "sidebar.right", isOn: $inspector) }
        }
        .navigationTitle(document.filename)
        .task {
            await model.load(url: configuration?.fileURL)
            await Benchmark.runIfRequested(model.controller)
        }
        .onChange(of: model.session.pose) { _, pose in model.controller?.rotation = pose.float3x3 }
        .onChange(of: undoManager, initial: true) { _, m in model.session.undoManager = m }
        .focusedSceneValue(\.viewer, model.controller)
        .focusedSceneValue(\.session, model.session)
        .focusedSceneValue(\.exporter, model.wiring?.exporter)
        .focusedSceneValue(\.removal, model.wiring?.removal)
        // Esc ends Remove fins mode wherever the focus is (the viewer handles it too when it has the keys)
        .onExitCommand { model.wiring?.escape() }
    }
}
