import SupportFinsCore
import SwiftUI

extension ToolbarSlot {
    static let suggest = ToolbarSlot(id: "suggest", title: "Suggest", systemImage: "wand.and.stars",
                                     help: "Rank the ways this part could sit on the bed.", order: 20)
}

/// Suggest (`ui/suggest.js`): the ranking popover and Model > Suggest Orientation. It exists once a single part
/// is on show (a file of several objects is turned as a whole, so it has no ranking of its own yet).
@MainActor
final class SuggestFeature: Feature {
    unowned let host: ViewerModel
    private(set) var model: SuggestModel?

    init(host: ViewerModel) { self.host = host }

    var toolbarSlots: [ToolbarSlot] { [.suggest] }

    func partDidShow(_ parts: [LoadedPart]) {
        model = SuggestModel(part: parts.singleCorePart, pose: PoseAccess.session(host.session))
        registerTools(in: host.toolbar)
    }

    func modelState() -> Any? { model?.kept.map { Kept(result: $0.result, picked: $0.picked) } }
    func restoreModelState(_ state: Any) { if let k = state as? Kept { model?.restore(k.result, picked: k.picked) } }
    private struct Kept { var result: SuggestResult; var picked: Int? }

    func registerTools(in toolbar: ToolbarRegistry) {
        guard let controller = host.controller, let suggest = model else { toolbar.unregister(.suggest); return }
        toolbar.register(.suggest) { [host] in
            SuggestPopover(model: suggest) {
                suggest.rank(thresholdDegrees: controller.thresholdDegrees, optionsJSON: FinSettings().optionsJSON(),
                             currentBore: host.wiring?.build?.summary?.boreWalls ?? 0,
                             density: FinReadout.materialDensity)
            }
        }
    }

    var modifiers: [WindowModifier] {
        [.onChange("suggest-pose", of: { self.host.session.pose }) { _, _ in self.model?.poseChanged() }]
    }

    static func menus(_ f: SuggestFeature?) -> [MenuSection] {
        let toolbar = f?.host.toolbar
        // opens its popover in the view toolbar (`ToolbarRegistry.open`); only a single part has it
        return [MenuSection(menu: .model, order: 10, entries: [
            .divider,
            .item(JS.t("Suggest Orientation"), enabled: toolbar?.badges[.suggest] != nil) { toolbar?.open = .suggest },
        ])]
    }
}
