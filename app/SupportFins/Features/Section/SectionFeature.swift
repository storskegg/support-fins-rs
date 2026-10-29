import SwiftUI
import Viewer

extension ToolbarSlot {
    static let section = ToolbarSlot(
        id: "section", title: "Section", systemImage: "rectangle.lefthalf.filled",
        help: "Cut the view with a plane to see and draw inside the part. Drag the arrow or the sheet (or Shift + scroll) to push the cut, the rings to tilt it. Click again to turn it off. View only: the export is whole.",
        order: 35)
}

/// Section view (`ui/section.js`): a plane the user moves and tilts that hides everything on one side. The
/// toolbar's Section tool badge with its popover (Ghost / Hide, Flip, Re-aim) and View > Section View. The plane,
/// its handle, the clipping and the ghost are the viewer's (`SectionTool`, `Section/SectionRenderer.swift`).
@MainActor
final class SectionFeature: Feature {
    unowned let host: ViewerModel
    let model: SectionModel?

    init(host: ViewerModel) {
        self.host = host
        model = host.controller.map { SectionModel(tool: $0.section) }
    }

    var toolbarSlots: [ToolbarSlot] { [.section] }

    func registerTools(in toolbar: ToolbarRegistry) {
        guard let model else { return }
        toolbar.registerTool(.section, isOn: { model.isOn }, setOn: { model.setOn($0) }) { SectionPopover(model: model) }
    }

    /// Switch the cut on or off from outside its badge. A tool that goes off folds its popover (`render`,
    /// `ui/toolbar.js:38`: `if (b.setOn && !on && openId === id) openId = null`), so it can't come back open with
    /// the next switch on.
    func setOn(_ on: Bool) {
        model?.setOn(on)
        if model?.isOn != true, host.toolbar.open == .section { host.toolbar.fold() }
    }

    /// A new part: the old part's cut means nothing on it (`resetPartState`, `ui/part.js:157`).
    func partWillShow() { setOn(false) }

    static func menus(_ f: SectionFeature?) -> [MenuSection] {
        let model = f?.model
        return [MenuSection(menu: .view, order: 20, entries: [
            .divider,
            .check("Section View",
                   help: "Cut the view with a plane to see and draw inside the part. View only: the export is whole.",
                   enabled: f?.host.hasPart == true,
                   isOn: Binding(get: { model?.isOn ?? false }, set: { f?.setOn($0) })),
        ])]
    }
}
