import Foundation
import Observation
import Viewer

/// The Section popover's state (`web/ui/section.js`): the plane and its handle are the viewer's (`SectionTool`);
/// this adds what the web remembers, Ghost or Hide (`sf.sectionGhost`, `ui/section.js:34-36, 386`).
@MainActor
@Observable
final class SectionModel {
    let tool: SectionTool
    @ObservationIgnored private let defaults: UserDefaults

    static let ghostKey = "sf.sectionGhost"

    init(tool: SectionTool, defaults: UserDefaults = .standard) {
        self.tool = tool
        self.defaults = defaults
        // Ghost unless the stored value says Hide (`localStorage.getItem(GHOST_KEY) !== '0'`)
        tool.ghost = defaults.object(forKey: Self.ghostKey) == nil ? true : defaults.bool(forKey: Self.ghostKey)
    }

    var isOn: Bool { tool.isOn }
    var ghost: Bool { tool.ghost }

    /// The Ghost / Hide buttons (`section-ghost`, `section-hide`).
    func setGhost(_ on: Bool) {
        tool.ghost = on
        defaults.set(on, forKey: Self.ghostKey)
    }

    /// The Section badge and View > Section View (`setSection`).
    func setOn(_ on: Bool) { tool.setOn(on) }
    /// `section-flip`: show the other side of the cut instead.
    func flip() { tool.flip() }
    /// `section-reset` (Re-aim): aim the cut at the current view again, through the part's centre.
    func reaim() { tool.aimAtView() }
}
