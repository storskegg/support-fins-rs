import SwiftUI
import Viewer

extension ToolbarSlot {
    static let rotate = ToolbarSlot(id: "rotate", title: "Rotate", systemImage: "rotate.3d",
                                    help: "Turn the part: 90° steps, Reset, Lay a face flat and the rotate rings.", order: 10)
}

/// Rotate and lay-face (`ui/pose.js`): the Rotate popover, the rotate rings' edits, the lay-a-face mode with its
/// hint, the Model menu's pose commands and Show Rotate Rings.
@MainActor
final class RotateFeature: Feature {
    unowned let host: ViewerModel
    /// Lay a face flat (`ui/pose.js`).
    let layFace: LayFaceModel
    /// The Rotate popover's model (`ui/pose.js`).
    let rotate: RotateModel
    private let ringEdits: RingEdits?

    init(host: ViewerModel) {
        self.host = host
        layFace = LayFaceModel(session: host.session, controller: host.controller)
        let rotate = RotateModel(session: host.session, controller: host.controller)
        self.rotate = rotate
        rotate.registry = host.toolbar
        ringEdits = host.controller.map { RingEdits(session: host.session, controller: $0) }
        ringEdits?.onEvent = { rotate.ring($0) }
    }

    var toolbarSlots: [ToolbarSlot] { [.rotate] }

    func registerTools(in toolbar: ToolbarRegistry) {
        toolbar.register(.rotate) { [rotate, layFace] in RotatePopover(model: rotate, layFace: layFace) }
    }

    func partWillShow() { layFace.cancel() }

    var overlays: [WindowOverlay] {
        [WindowOverlay("lay-face-hint", order: 50, alignment: .top) { LayFaceHint(model: self.layFace, clashBannerUp: self.host.features[PlatesFeature.self]?.board.clashing == true) }]
    }

    var modifiers: [WindowModifier] {
        [
            // a turn from anywhere (undo, redo, Suggest) disarms lay-flat
            .onChange("lay-face-disarm-on-pose", of: { self.host.session.pose }) { _, _ in self.layFace.cancel() },
            // arming folds the popover: the next click is on the part, which would only dismiss it
            .onChange("fold-on-arm", of: { self.layFace.armed }) { _, armed in if armed { self.host.toolbar.fold() } },
        ]
    }

    var escapeHandlers: [EscapeHandler] {
        [EscapeHandler(id: "lay-face", priority: EscapeHandler.pointerMode) {
            guard self.layFace.armed else { return false }
            self.layFace.cancel()
            return true
        }]
    }

    static func menus(_ f: RotateFeature?) -> [MenuSection] {
        let session = f?.host.session, layFace = f?.layFace, rotate = f?.rotate
        let hasPart = f?.host.hasPart == true
        return [
            MenuSection(menu: .model, order: 0, entries:
                // ⌘R, ⌥⌘R, ⌃⌘R (native; the web has no shortcut)
                Pose.Axis.allCases.map { axis in
                    .item(JS.t("Rotate 90° About {axis}", ["axis": axis.label]), shortcut: "r", modifiers: axis.menuModifiers, enabled: hasPart) { session?.rotate90(about: axis) }
                }
                + [
                    .item(JS.t("Reset Rotation"), enabled: hasPart) { session?.resetPose() },
                    .divider,
                    // `lay-face` (ui/pose.js:221): arms the lay-a-face pointer mode; a second choice disarms it
                    .check(JS.t("Lay a Face Flat"), enabled: layFace?.canArm == true,
                           isOn: Binding(get: { layFace?.armed ?? false }, set: { _ in layFace?.toggle() })),
                ]),
            // `show-rings` (ui/pose.js:161): the rotate rings, remembered
            MenuSection(menu: .view, order: 0, entries: [
                .check(JS.t("Show Rotate Rings"),
                       help: JS.t("The rotate rings around the part. Off hides them; Rotate 90°, Reset and Lay a face flat still turn the part."),
                       isOn: Binding(get: { rotate?.showRings ?? true }, set: { rotate?.showRings = $0 })),
            ]),
        ]
    }
}

private extension Pose.Axis {
    /// ⌘R rotates about X, ⌥⌘R about Y, ⌃⌘R about Z (docs/UI-MAP.md section 7).
    var menuModifiers: EventModifiers {
        switch self {
        case .x: .command
        case .y: [.command, .option]
        case .z: [.command, .control]
        }
    }
}
