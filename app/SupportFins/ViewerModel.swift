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
    /// The Rotate popover's model (`ui/pose.js`).
    let rotate: RotateModel

    init(loader: any PartLoading = CoreLoader()) {
        self.loader = loader
        controller = ViewerController()
        let rotate = RotateModel(session: session, controller: controller)
        self.rotate = rotate
        if controller == nil { status = "This Mac has no usable Metal device." }
        wiring = controller.map { BuildWiring(session: session, controller: $0) }
        ringEdits = controller.map { RingEdits(session: session, controller: $0) }
        ringEdits?.onEvent = { rotate.ring($0) }
    }

    func load(url: URL?) async {
        guard let controller else { return }
        do {
            let parts = try await loader.load(url: url, device: controller.renderer.device)
            guard let first = parts.first else { status = "The file has no mesh."; return }
            session.replaceParts(parts.compactMap { $0.mesh as? CorePart })
            controller.rotation = session.pose.float3x3
            wiring?.attach()
            controller.show(parts.map(\.mesh), name: parts.count == 1 ? first.name : "\(url?.lastPathComponent ?? first.name) (\(parts.count) objects)")
            status = nil
            makeTools(parts, controller: controller)
        } catch {
            status = error.localizedDescription
        }
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
