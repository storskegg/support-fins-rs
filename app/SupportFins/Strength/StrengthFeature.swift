import SwiftUI
import Viewer

extension ToolbarSlot {
    static let strength = ToolbarSlot(id: "strength", title: "Strength", systemImage: "arrow.up.and.down.and.arrow.left.and.right",
                                      help: "Strength arrow: which way is the part loaded in use?", order: 30)
}

/// Strength arrow (`ui/strength.js`): the load popover and Model > Strength Arrow…, for a single part.
@MainActor
final class StrengthFeature: Feature {
    unowned let host: ViewerModel
    private(set) var model: StrengthModel?

    init(host: ViewerModel) { self.host = host }

    var toolbarSlots: [ToolbarSlot] { [.strength] }

    func partDidShow(_ parts: [LoadedPart]) {
        let strength = StrengthModel(part: parts.singleCorePart, pose: PoseAccess.session(host.session))
        if let controller = host.controller {
            strength.threshold = controller.thresholdDegrees
            strength.loadChanged = { [weak controller] in controller?.loadDirection = $0.map { SIMD3<Float>(Float($0.x), Float($0.y), Float($0.z)) } }
        }
        model = strength
        registerTools(in: host.toolbar)
    }

    func registerTools(in toolbar: ToolbarRegistry) {
        guard host.controller != nil, let strength = model else { toolbar.unregister(.strength); return }
        toolbar.register(.strength, isLit: { strength.isLit }) { StrengthPopover(model: strength) }
    }

    var modifiers: [WindowModifier] {
        [
            .onChange("strength-pose", of: { self.host.session.pose }) { _, _ in self.model?.poseChanged() },
            .onChange("strength-threshold", of: { self.host.controller?.thresholdDegrees }) { _, t in
                if let t { self.model?.threshold = t }
            },
        ]
    }

    static func menus(_ f: StrengthFeature?) -> [MenuSection] {
        let toolbar = f?.host.toolbar
        return [MenuSection(menu: .model, order: 20, entries: [
            .item("Strength Arrow…", enabled: toolbar?.badges[.strength] != nil) { toolbar?.open = .strength },
        ])]
    }
}
