import Observation
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
    @ObservationIgnored private let loader: any PartLoading

    init(loader: any PartLoading = CoreLoader()) {
        self.loader = loader
        controller = ViewerController()
        if controller == nil { status = "This Mac has no usable Metal device." }
    }

    func load(url: URL?) async {
        guard let controller else { return }
        do {
            let parts = try await loader.load(url: url, device: controller.renderer.device)
            guard let first = parts.first else { status = "The file has no mesh."; return }
            session.replaceParts(parts.compactMap { $0.mesh as? CorePart })
            controller.rotation = session.pose.float3x3
            controller.show(parts.map(\.mesh), name: parts.count == 1 ? first.name : "\(url?.lastPathComponent ?? first.name) (\(parts.count) objects)")
            status = nil
        } catch {
            status = error.localizedDescription
        }
    }
}
