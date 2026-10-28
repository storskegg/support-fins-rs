import SwiftUI
import Viewer

extension ToolbarSlot {
    static let view = ToolbarSlot(id: "view", title: "View", systemImage: "eye",
                                  help: "Camera views, the mouse preset and the print layers around the part.", order: 40)
}

/// View (`ui/navigation.js`, `ui/viewcube.js`): the camera views, the Mouse preset, the view cube and the floating
/// toolbar they sit over, the View menu's commands and the debug frame-time readout.
@MainActor
final class ViewFeature: Feature {
    unowned let host: ViewerModel
    /// The View popover's Mouse preset (`ui/navigation.js`).
    let settings: ViewSettings

    init(host: ViewerModel) {
        self.host = host
        settings = ViewSettings(controller: host.controller)
    }

    var toolbarSlots: [ToolbarSlot] { [.view] }

    func registerTools(in toolbar: ToolbarRegistry) {
        guard let controller = host.controller else { return }
        toolbar.register(.view) { [settings] in ViewPopover(controller: controller, settings: settings) }
    }

    var overlays: [WindowOverlay] {
        var layers: [WindowOverlay] = []
        #if DEBUG
        layers.append(WindowOverlay("frame-time", order: 10, ignoresSafeArea: true) {
            if let controller = self.host.controller, controller.showsFrameTime { FrameOverlay(controller: controller) }
        })
        #endif
        layers.append(WindowOverlay("view-cube-and-toolbar", order: 60, alignment: .bottom) {
            if let controller = self.host.controller, self.host.hasPart {
                // the view cube sits above the toolbar's left end, and steps aside for a popover (`ui/viewcube.js`)
                let toolbar = self.host.toolbar
                VStack(alignment: .leading, spacing: 8) {
                    ViewCubeView(controller: controller)
                        .opacity(toolbar.open == nil && toolbar.flash == nil ? 1 : 0)
                        .allowsHitTesting(toolbar.open == nil && toolbar.flash == nil)
                    ViewToolbar(session: self.host.session, registry: toolbar)
                }
            }
        })
        return layers
    }

    /// View menu: the camera views of docs/UI-MAP.md (the web app's View popover) with ⌘0..⌘6.
    static func menus(_ f: ViewFeature?) -> [MenuSection] {
        let viewer = f?.host.controller, settings = f?.settings
        var entries: [MenuEntry] = [
            // a native improvement: the fins follow a ring drag; off, they are built when it ends, as in the web
            .check("Rebuild While Dragging",
                   help: "Rebuild the fins while a rotate ring is dragged, so they follow the part. Off builds them when the drag ends.",
                   isOn: Binding(get: { settings?.rebuildWhileDragging ?? true }, set: { settings?.rebuildWhileDragging = $0 })),
            .item("Frame Part", shortcut: "0") { viewer?.frame() },
            // the view cube's house (`ui/viewcube.js`): the starting angle, keeping the target and the zoom. The web
            // has no Frame Part, so Home sits beside it rather than replacing it.
            .item("Home", help: "Home view: front right, from above. Keeps the zoom.") { viewer?.goHome() },
            // `show-layers` (ui/strength.js): the horizontal print layers around the part
            .check("Show Print Layers", help: "Show the horizontal print layers around the part.",
                   isOn: Binding(get: { viewer?.showsLayers ?? false }, set: { viewer?.showsLayers = $0 })),
        ]
        #if DEBUG
        // docs/UI-MAP.md: the debug-only frame-time overlay (the web's `#fps`)
        entries.append(.check("Show Frame Time", isOn: Binding(get: { viewer?.showsFrameTime ?? false }, set: { viewer?.showsFrameTime = $0 })))
        #endif
        entries.append(.divider)
        for (i, v) in CameraView.allCases.enumerated() {
            entries.append(.item(v.label, shortcut: KeyEquivalent(Character("\(i + 1)"))) { viewer?.apply(v) })
        }
        return [MenuSection(menu: .view, order: 10, entries: entries)]
    }
}
