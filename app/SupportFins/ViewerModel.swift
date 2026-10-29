import Observation
import SupportFinsCore
import SwiftUI
import Viewer

/// One window's viewer: the controller and the load of its part.
@MainActor
@Observable
final class ViewerModel {
    let controller: ViewerController?
    /// The window's state; its settings start from what the web remembers across visits (`SettingsStore`).
    let session = PartSession(settings: SettingsStore().load())
    var status: String?
    /// The fins: builds, removal and export for the first object of the file.
    private(set) var wiring: BuildWiring?
    @ObservationIgnored private let loader: any PartLoading
    /// The floating toolbar's badge slots; features register their popovers here.
    let toolbar = ToolbarRegistry()
    /// The window's features (`Features.all`): each owns its models and contributes to the window and the menus.
    @ObservationIgnored private(set) var features: FeatureSet!

    init(loader: any PartLoading = CoreLoader()) {
        self.loader = loader
        controller = ViewerController()
        if controller == nil { status = JS.t("This Mac has no usable Metal device.") }
        wiring = controller.map { [session] in
            BuildWiring(session: session, controller: $0,
                        liveDrag: { [weak self] in self?.features[ViewFeature.self]?.settings.rebuildWhileDragging ?? true })
        }
        features = FeatureSet(host: self)
    }

    /// The window closed (SwiftUI drops its model): what the features and the build started stops now, rather than
    /// when each Task next wakes to find the window gone (the features hold it weakly).
    isolated deinit {
        features?.close()
        wiring?.close()
    }

    /// The object picker, while a file with several objects waits for a choice (`pickObjects`, `ui/io.js:140`).
    var picker: ObjectPickerModel?
    /// A file was tried in this window (the empty window then shows only the button, `#drop.compact`).
    private(set) var hasOpened = false
    private(set) var isLoading = false
    /// The file being read, for the "reading STEP…" overlay (`parseStep`, `ui/io.js:313`).
    private(set) var loadingURL: URL?
    /// The name of the file on show, which can differ from the document's when a file is opened in place.
    var openedName: String?
    var hasPart: Bool { (controller?.partCount ?? 0) > 0 }

    /// No part, nothing loading and no picker open: the intro (or the compact button) fills the window.
    var showsEmptyState: Bool { controller != nil && !hasPart && !isLoading && picker == nil }

    /// The part being loaded goes beside the plate's models instead of in place of the one on show (Add Model to
    /// Plate, `ui/io.js` `loadFile(file, true)`): the `Plates` feature parks the model on show when the part arrives.
    /// Cleared once the part is shown, or the load ends without one.
    private(set) var addingModel = false

    func load(url: URL?, adding: Bool = false) async {
        guard let controller else { return }
        hasOpened = true
        isLoading = true
        addingModel = adding && hasPart
        loadingURL = url
        defer { isLoading = false; loadingURL = nil }
        do {
            let parts = try await loader.load(url: url, device: controller.renderer.device)
            guard !parts.isEmpty else { addingModel = false; status = JS.t("The file has no mesh."); return }
            // a plate of several goes to the picker; cancelling it keeps whatever is on show (`io.js:200-205`)
            guard parts.count > 1 else { show(parts, url: url); return }
            let picker = ObjectPickerModel(objects: parts.map {
                let d = $0.mesh.bounds.max - $0.mesh.bounds.min
                return (name: $0.name, triangles: $0.mesh.faceCount, size: [Double(d.x), Double(d.y), Double(d.z)])
            })
            picker.onCancel = { [weak self] in self?.picker = nil; self?.addingModel = false }
            picker.onLoad = { [weak self] chosen in
                guard let self else { return }
                self.picker = nil
                guard chosen.count > 1 else { self.show([parts[chosen[0]]], url: url); return }
                Task { await self.showMerged(chosen.map { parts[$0] }, of: parts.count, url: url) }
            }
            self.picker = picker
        } catch {
            addingModel = false
            status = error.localizedDescription
        }
    }

    /// "Merge N & load": the chosen objects as one part (`merge_parts`; `mergeObjectPositions`, `ui/io.js:120`).
    private func showMerged(_ chosen: [LoadedPart], of total: Int, url: URL?) async {
        guard let controller else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let cores = chosen.compactMap { ($0.mesh as? CorePart)?.part }
            guard cores.count == chosen.count else { addingModel = false; status = JS.t("These objects cannot be merged."); return }
            let merged = try await Task.detached(priority: .userInitiated) { try mergeParts(parts: cores, objectCount: UInt64(total)) }.value
            guard let core = CorePart(part: merged, device: controller.renderer.device) else { addingModel = false; status = JS.t("The merged mesh could not be shared with the GPU."); return }
            show([LoadedPart(name: core.info.name, mesh: core)], url: url)
        } catch {
            addingModel = false
            status = error.localizedDescription
        }
    }

    /// The part on show again at another size (Scale, `ui/scale.js setScale`): `setPart` with the pose kept. Like a
    /// new file, the walls, removed fins, section cut and undo start over.
    func reshow(_ core: CorePart) {
        show([LoadedPart(name: core.info.name, mesh: core)], url: nil, keepingPose: true)
    }

    private func show(_ parts: [LoadedPart], url: URL?, keepingPose: Bool = false) {
        guard let controller, let first = parts.first else { return }
        let pose = session.pose
        features.partWillShow()
        session.replaceParts(parts.compactMap { $0.mesh as? CorePart })
        if keepingPose { session.pose = pose }
        controller.rotation = session.pose.float3x3
        wiring?.attach()
        controller.show(parts.map(\.mesh), name: first.name)
        if !keepingPose { openedName = url?.lastPathComponent ?? first.name }
        status = nil
        features.partDidShow(parts)
        addingModel = false
    }
}
