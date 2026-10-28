import SwiftUI
import Viewer

struct ContentView: View {
    let document: PartDocument
    @Environment(\.documentConfiguration) private var configuration
    @State private var model = ViewerModel()
    @State private var inspector = true
    /// The floating toolbar's badge slots; tracks register their popovers through the environment.
    @State private var toolbar = ToolbarRegistry()
    @Environment(\.undoManager) private var undoManager

    var body: some View {
        main
            .focusedSceneValue(\.viewer, model.controller)
            .focusedSceneValue(\.session, model.session)
            .focusedSceneValue(\.layFace, model.layFace)
            .focusedSceneValue(\.rotate, model.rotate)
            .focusedSceneValue(\.toolbar, toolbar)
            .focusedSceneValue(\.exporter, model.wiring?.exporter)
            .focusedSceneValue(\.removal, model.wiring?.removal)
            // Esc ends Remove fins mode wherever the focus is (the viewer handles it too when it has the keys)
            .onExitCommand { model.layFace.cancel(); model.wiring?.escape() }
            .environment(model.session)
    }

    private var main: some View {
        ZStack(alignment: .topLeading) {
            if let controller = model.controller {
                ViewerViewport(controller: controller)
                    .ignoresSafeArea()
                    #if DEBUG
                    .overlay(alignment: .topLeading) { FrameOverlay(controller: controller) }
                    #endif
            }
            if model.showsEmptyState {
                EmptyWindowView(compact: model.hasOpened) { OpenFlow.importModel(into: model) }
            }
            if let status = model.status {
                Text(status).padding(8).background(.regularMaterial, in: .rect(cornerRadius: 8)).padding()
            }
            if let w = model.wiring, w.spinnerVisible { BuildSpinner() }
        }
        .overlay(alignment: .top) { LayFaceHint(model: model.layFace) }
        .overlay(alignment: .bottom) {
            if let controller = model.controller, model.hasPart {
                // the view cube sits above the toolbar's left end, and steps aside for a popover (`ui/viewcube.js`)
                VStack(alignment: .leading, spacing: 8) {
                    ViewCubeView(controller: controller)
                        .opacity(toolbar.open == nil && toolbar.flash == nil ? 1 : 0)
                        .allowsHitTesting(toolbar.open == nil && toolbar.flash == nil)
                    ViewToolbar(session: model.session, registry: toolbar)
                }
            }
        }
        .environment(toolbar)
        // track D's popovers plug into the toolbar's Suggest and Strength slots once a single part is on show
        .onChange(of: model.suggest.map(ObjectIdentifier.init), initial: true) { _, _ in registerTools() }
        .onChange(of: model.session.pose) {
            model.layFace.cancel()          // a turn from anywhere (undo, redo, Suggest) disarms lay-flat
            model.suggest?.poseChanged()
            model.strength?.poseChanged()
        }
        // arming folds the popover: the next click is on the part, which would only dismiss it
        .onChange(of: model.layFace.armed) { _, armed in if armed { toolbar.fold() } }
        .onChange(of: model.controller?.thresholdDegrees) { _, t in if let t { model.strength?.threshold = t } }
        .inspector(isPresented: $inspector) {
            if let controller = model.controller {
                InspectorView(controller: controller, settings: model.session.settingsBinding,
                              readout: AnyView(ReadoutSections(controller: controller, session: model.session, build: model.wiring?.build)))
                    .inspectorColumnWidth(min: 240, ideal: 280, max: 360)
            }
        }
        .toolbar {
            ToolbarItem { if let controller = model.controller { VolumePicker(controller: controller) } }
            ToolbarItem { OverhangSlider(session: model.session) }
            ToolbarItem { FinsToggle(session: model.session) }
            ToolbarItem { ExportMenu(model: model.wiring?.exporter) }
            ToolbarItem { Toggle("Inspector", systemImage: "sidebar.right", isOn: $inspector) }
        }
        .openFlow(model)
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

    private func registerTools() {
        model.rotate.registry = toolbar
        if let controller = model.controller { toolbar.register(.view) { ViewPopover(controller: controller, settings: model.view) } }
        toolbar.register(.rotate) { RotatePopover(model: model.rotate, layFace: model.layFace) }
        guard let controller = model.controller, let suggest = model.suggest, let strength = model.strength else {
            toolbar.unregister(.suggest); toolbar.unregister(.strength); return
        }
        toolbar.register(.suggest) {
            SuggestPopover(model: suggest) {
                suggest.rank(thresholdDegrees: controller.thresholdDegrees, optionsJSON: FinSettings().optionsJSON(),
                             currentBore: model.wiring?.build?.summary?.boreWalls ?? 0,
                             density: FinReadout.materialDensity)
            }
        }
        toolbar.register(.strength, isLit: { strength.isLit }) { StrengthPopover(model: strength) }
    }
}
