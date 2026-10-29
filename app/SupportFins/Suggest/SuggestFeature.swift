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
    weak let host: ViewerModel?
    private(set) var model: SuggestModel?

    init(host: ViewerModel) { self.host = host }

    var toolbarSlots: [ToolbarSlot] { [.suggest] }

    func partDidShow(_ parts: [LoadedPart]) {
        guard let host else { return }
        model = SuggestModel(part: parts.singleCorePart, pose: PoseAccess.session(host.session))
        registerTools(in: host.toolbar)
    }

    func modelState() -> Any? { model?.kept.map { Kept(result: $0.result, picked: $0.picked) } }
    func restoreModelState(_ state: Any) { if let k = state as? Kept { model?.restore(k.result, picked: k.picked) } }
    private struct Kept { var result: SuggestResult; var picked: Int? }

    func registerTools(in toolbar: ToolbarRegistry) {
        guard host?.controller != nil, let suggest = model else { toolbar.unregister(.suggest); return }
        // the toolbar is the window's: a strong host here would keep the window alive for ever (C-01)
        toolbar.register(.suggest) { [weak host] in
            SuggestPopover(model: suggest) { if let host { Self.rank(suggest, in: host) } }
        }
    }

    /// The button's click: rank under the window's current settings, the build's options and tunables, as the page's
    /// Worker scores with `supportTunables()` (`ui/suggest.js`: its gap, pad, nozzle and wall lines).
    func rank() { if let model, let host { Self.rank(model, in: host) } }

    private static func rank(_ suggest: SuggestModel, in host: ViewerModel) {
        guard let controller = host.controller else { return }
        let settings = host.session.settings
        suggest.rank(thresholdDegrees: controller.thresholdDegrees, optionsJSON: settings.optionsJSON(),
                     tunablesJSON: settings.tunablesJSON(),
                     currentBore: host.wiring?.build?.summary?.boreWalls ?? 0,
                     density: FinReadout.materialDensity)
    }

    var modifiers: [WindowModifier] {
        [.onChange("suggest-pose", of: { self.host?.session.pose }) { _, _ in self.model?.poseChanged() }]
    }

    static func menus(_ f: SuggestFeature?) -> [MenuSection] {
        let toolbar = f?.host?.toolbar
        // opens its popover in the view toolbar (`ToolbarRegistry.open`); only a single part has it
        return [MenuSection(menu: .model, order: 10, entries: [
            .divider,
            .item(JS.t("Suggest Orientation"), enabled: toolbar?.badges[.suggest] != nil) { toolbar?.open = .suggest },
        ])]
    }
}
