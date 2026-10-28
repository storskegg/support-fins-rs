import Foundation
import Observation
import Viewer

/// The View popover's remembered setting: the Mouse preset (`ui/navigation.js`), kept under the web's storage
/// key `sf.navPreset` and applied to the viewer. Show layers is the viewer's own `showsLayers`, shared with
/// View > Show Print Layers; the camera views are `ViewerController.apply`, shared with the View menu.
@MainActor
@Observable
final class ViewSettings {
    @ObservationIgnored private weak var controller: ViewerController?
    @ObservationIgnored private let defaults: UserDefaults

    var navPreset: NavPreset {
        didSet {
            defaults.set(navPreset.rawValue, forKey: NavPreset.storageKey)
            controller?.navPreset = navPreset
        }
    }

    init(controller: ViewerController?, defaults: UserDefaults = .standard) {
        self.controller = controller
        self.defaults = defaults
        // an unknown stored key falls back to Default (`hasNavPreset`, ui/navigation.js:29)
        navPreset = NavPreset(storedKey: defaults.string(forKey: NavPreset.storageKey))
        controller?.navPreset = navPreset
    }
}
