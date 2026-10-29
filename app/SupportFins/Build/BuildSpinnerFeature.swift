import SwiftUI

/// The progress overlay (`#spinner`) while the fins build.
@MainActor
final class BuildSpinnerFeature: Feature {
    weak let host: ViewerModel?

    init(host: ViewerModel) { self.host = host }

    var overlays: [WindowOverlay] {
        [WindowOverlay("build-spinner", order: 40) {
            if let w = self.host?.wiring, w.spinnerVisible { BuildSpinner() }
        }]
    }
}
