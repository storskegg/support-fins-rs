import SwiftUI
import Viewer
import simd

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
    weak let host: ViewerModel?
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
        if model?.isOn != true, let toolbar = host?.toolbar, toolbar.open == .section { toolbar.fold() }
    }

    /// A new part: the old part's cut means nothing on it (`resetPartState`, `ui/part.js:157`).
    func partWillShow() {
        leaving = model?.tool.heldCut
        setOn(false)
    }

    func partDidShow(_ parts: [LoadedPart]) { leaving = nil }

    /// The cut a part change just switched off, until the new part is on show: Add model parks the model on show from
    /// `partWillShow` too (`PlatesFeature`, after this feature's), and its cut goes with it (`addSlot` -> `park`).
    private var leaving: simd_float4x4?

    /// A model parked on its plate (`park`, `ui/models.js:68`) keeps its cut, where it sits on the part, or nothing
    /// when the cut is off (`sectionState`); the part goes off the page and the cut with it (`stashPart` ->
    /// `resetPartState` -> `setSection(false)`, `ui/part.js:157`).
    private struct Kept { var inPart: simd_float4x4 }
    func modelState() -> Any? {
        let kept = (model?.tool.heldCut ?? leaving).map(Kept.init)
        leaving = nil
        setOn(false)
        return kept
    }

    /// The model is on show again (`unpark`, `ui/models.js:104`): its cut back on, where it was on the part
    /// (`restoreSection`). Its popover stays as it is, as `setSection(true)` leaves it.
    func restoreModelState(_ state: Any) {
        guard let kept = state as? Kept else { return }
        model?.tool.restore(kept.inPart)
    }

    static func menus(_ f: SectionFeature?) -> [MenuSection] {
        let model = f?.model
        return [MenuSection(menu: .view, order: 20, entries: [
            .divider,
            .check(JS.t("Section View"),
                   help: JS.t("Cut the view with a plane to see and draw inside the part. Drag the arrow or the sheet (or Shift + scroll) to push the cut, the rings to tilt it. Click again to turn it off. View only: the export is whole."),
                   enabled: f?.host?.hasPart == true,
                   isOn: Binding(get: { model?.isOn ?? false }, set: { f?.setOn($0) })),
        ])]
    }
}
