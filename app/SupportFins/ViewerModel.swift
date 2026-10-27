import Observation
import SwiftUI
import Viewer

/// One window's viewer: the controller and the load of its part.
@MainActor
@Observable
final class ViewerModel {
    let controller: ViewerController?
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
            controller.show(parts.map(\.mesh), name: parts.count == 1 ? first.name : "\(url?.lastPathComponent ?? first.name) (\(parts.count) objects)")
            status = nil
        } catch {
            status = error.localizedDescription
        }
    }
}
