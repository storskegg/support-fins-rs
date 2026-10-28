import SwiftUI
import Viewer

struct ContentView: View {
    let document: PartDocument
    @Environment(\.documentConfiguration) private var configuration
    @State private var model = ViewerModel()
    @State private var inspector = true
    @Environment(\.undoManager) private var undoManager
    @State private var suggestOpen = false
    @State private var strengthOpen = false

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
        // the Suggest and Strength badges: a stand-in for the view toolbar's slots (Toolbar/BadgeBar.swift)
        .overlay(alignment: .bottom) {
            if let controller = model.controller, let suggest = model.suggest, let strength = model.strength {
                BadgeBar {
                    Badge(title: "Suggest", help: "Rank the ways this part could sit on the bed.", open: $suggestOpen) {
                        SuggestPopover(model: suggest) {
                            suggest.rank(thresholdDegrees: controller.thresholdDegrees, optionsJSON: FinSettings().optionsJSON(),
                                         currentBore: 0, density: 1.24)
                        }
                    }
                    Badge(title: "Strength", help: "Strength arrow: which way is the part loaded in use?", lit: strength.isLit,
                          open: $strengthOpen) { StrengthPopover(model: strength) }
                }
                .padding(.bottom, 14)
                .onChange(of: model.session.pose) {
                    suggest.poseChanged()
                    strength.poseChanged()
                }
                .onChange(of: controller.thresholdDegrees) { strength.threshold = controller.thresholdDegrees }
            }
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
