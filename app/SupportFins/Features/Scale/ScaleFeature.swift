import SwiftUI
import SupportFinsCore
import Viewer

extension ToolbarSlot {
    static let scale = ToolbarSlot(id: "scale-badge", title: "Scale", systemImage: "arrow.up.left.and.arrow.down.right",
                                   help: "Scale the part by a percent.", order: 15)
}

/// Scale (`ui/scale.js`): the Scale popover, the percent on its badge and Model > Scale…, for a single part.
@MainActor
final class ScaleFeature: Feature {
    unowned let host: ViewerModel
    let model = ScaleModel()

    init(host: ViewerModel) {
        self.host = host
        model.apply = { [weak host] part in
            guard let host, let controller = host.controller,
                  let core = CorePart(part: part, device: controller.renderer.device) else { return }
            host.reshow(core)
        }
    }

    var toolbarSlots: [ToolbarSlot] { [.scale] }

    func partDidShow(_ parts: [LoadedPart]) {
        // a reload of our own keeps the scale and the base it was taken from
        if !model.rescaling { model.newPart(parts.singleCorePart) }
        registerTools(in: host.toolbar)
    }

    func registerTools(in toolbar: ToolbarRegistry) {
        guard host.controller != nil, model.hasPart else { toolbar.unregister(.scale); return }
        toolbar.register(.scale, isLit: { [model] in model.isScaled }, detail: { [model] in model.badgeSuffix }) {
            ScalePopover(model: self.model)
        }
    }

    static func menus(_ f: ScaleFeature?) -> [MenuSection] {
        let toolbar = f?.host.toolbar
        return [MenuSection(menu: .model, order: 5, entries: [
            .divider,
            .item("Scale…", help: "Scale the part by a percent.", enabled: toolbar?.badges[.scale] != nil) { toolbar?.open = .scale },
        ])]
    }
}
