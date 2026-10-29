import Observation
import SwiftUI

/// What's new (`web/changelog.js`, `ui/whatsnew.js`, the card under the page's intro): the card itself is in the empty
/// window (`EmptyWindowView`, `WhatsNewCard`); this feature is Help > What's new, which opens the same list in a sheet.
/// Like the page, it shows nothing while nothing is released: the item is not in the menu.
@MainActor
final class WhatsNewFeature: Feature {
    unowned let host: ViewerModel
    let model = WhatsNewModel()

    init(host: ViewerModel) { self.host = host }

    var modifiers: [WindowModifier] {
        [WindowModifier("whatsnew-sheet") { content in AnyView(WhatsNewHost(model: self.model, content: content)) }]
    }

    static func menus(_ f: WhatsNewFeature?) -> [MenuSection] {
        entries(for: .bundled, show: { f?.model.show() }, enabled: f != nil)
    }

    /// The Help section for `changelog`: one item with a release to show, none without.
    static func entries(for changelog: Changelog, show: @escaping @MainActor () -> Void, enabled: Bool = true) -> [MenuSection] {
        guard let newest = changelog.newest else { return [] }
        return [MenuSection(menu: .help, order: 15, entries: [
            .item(JS.t("What's new"), help: "\(JS.t("Version")) \(newest.tag)", enabled: enabled, action: show),
        ])]
    }
}

@MainActor @Observable
final class WhatsNewModel {
    var isShowing = false
    func show() { isShowing = true }
}

private struct WhatsNewHost: View {
    @Bindable var model: WhatsNewModel
    let content: AnyView

    var body: some View {
        content.sheet(isPresented: $model.isShowing) { WhatsNewSheet(changelog: .bundled) }
    }
}
