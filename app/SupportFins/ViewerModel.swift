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
    /// The Suggest and Strength popovers' models, once a single part is on show (a file of several objects is
    /// turned as a whole, so it has no ranking or load arrow of its own yet).
    private(set) var suggest: SuggestModel?
    private(set) var strength: StrengthModel?
    @ObservationIgnored private let loader: any PartLoading
    @ObservationIgnored private var ringEdits: RingEdits?
    /// Lay a face flat (`ui/pose.js`).
    let layFace: LayFaceModel
    /// The Rotate popover's model (`ui/pose.js`).
    let rotate: RotateModel
    /// The View popover's Mouse preset (`ui/navigation.js`).
    let view: ViewSettings

    init(loader: any PartLoading = CoreLoader()) {
        self.loader = loader
        controller = ViewerController()
        layFace = LayFaceModel(session: session, controller: controller)
        let rotate = RotateModel(session: session, controller: controller)
        self.rotate = rotate
        view = ViewSettings(controller: controller)
        if controller == nil { status = "This Mac has no usable Metal device." }
        wiring = controller.map { BuildWiring(session: session, controller: $0) }
        ringEdits = controller.map { RingEdits(session: session, controller: $0) }
        ringEdits?.onEvent = { rotate.ring($0) }
    }

    /// The object picker, while a file with several objects waits for a choice (`pickObjects`, `ui/io.js:140`).
    var picker: ObjectPickerModel?
    /// A file was tried in this window (the empty window then shows only the button, `#drop.compact`).
    private(set) var hasOpened = false
    private(set) var isLoading = false
    /// The name of the file on show, which can differ from the document's when a file is opened in place.
    private(set) var openedName: String?
    var hasPart: Bool { (controller?.partCount ?? 0) > 0 }

    /// No part, nothing loading and no picker open: the intro (or the compact button) fills the window.
    var showsEmptyState: Bool { controller != nil && !hasPart && !isLoading && picker == nil }

    func load(url: URL?) async {
        guard let controller else { return }
        hasOpened = true
        isLoading = true
        defer { isLoading = false }
        do {
            let parts = try await loader.load(url: url, device: controller.renderer.device)
            guard !parts.isEmpty else { status = "The file has no mesh."; return }
            // a plate of several goes to the picker; cancelling it keeps whatever is on show (`io.js:200-205`)
            guard parts.count > 1 else { show(parts, url: url); return }
            let picker = ObjectPickerModel(objects: parts.map {
                let d = $0.mesh.bounds.max - $0.mesh.bounds.min
                return (name: $0.name, triangles: $0.mesh.faceCount, size: [Double(d.x), Double(d.y), Double(d.z)])
            })
            picker.onCancel = { [weak self] in self?.picker = nil }
            picker.onLoad = { [weak self] chosen in
                guard let self else { return }
                self.picker = nil
                guard chosen.count > 1 else { self.show([parts[chosen[0]]], url: url); return }
                Task { await self.showMerged(chosen.map { parts[$0] }, url: url) }
            }
            self.picker = picker
        } catch {
            status = error.localizedDescription
        }
    }

    /// "Merge N & load": the chosen objects as one part (`merge_parts`; `mergeObjectPositions`, `ui/io.js:120`).
    private func showMerged(_ chosen: [LoadedPart], url: URL?) async {
        guard let controller else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let cores = chosen.compactMap { ($0.mesh as? CorePart)?.part }
            guard cores.count == chosen.count else { status = "These objects cannot be merged."; return }
            let merged = try await Task.detached(priority: .userInitiated) { try mergeParts(parts: cores) }.value
            guard let core = CorePart(part: merged, device: controller.renderer.device) else { status = "The merged mesh could not be shared with the GPU."; return }
            show([LoadedPart(name: core.info.name, mesh: core)], url: url)
        } catch {
            status = error.localizedDescription
        }
    }

    private func show(_ parts: [LoadedPart], url: URL?) {
        guard let controller, let first = parts.first else { return }
        layFace.cancel()
        session.replaceParts(parts.compactMap { $0.mesh as? CorePart })
        controller.rotation = session.pose.float3x3
        wiring?.attach()
        controller.show(parts.map(\.mesh), name: first.name)
        openedName = url?.lastPathComponent ?? first.name
        status = nil
        makeTools(parts, controller: controller)
    }
}

extension ViewerModel {
    /// The tools that act on the part's pose (`ui/suggest.js`, `ui/strength.js`).
    fileprivate func makeTools(_ parts: [LoadedPart], controller: ViewerController) {
        let part = parts.count == 1 ? (parts[0].mesh as? CorePart)?.part : nil
        let pose = PoseAccess.session(session)
        suggest = SuggestModel(part: part, pose: pose)
        let strength = StrengthModel(part: part, pose: pose)
        strength.threshold = controller.thresholdDegrees
        strength.loadChanged = { [weak controller] in controller?.loadDirection = $0.map { SIMD3<Float>(Float($0.x), Float($0.y), Float($0.z)) } }
        self.strength = strength
    }
}
