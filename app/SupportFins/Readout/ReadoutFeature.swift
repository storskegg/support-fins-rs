import SwiftUI

/// The inspector's readout: the stats and status cards on top, with the lines other features add to the status card.
@MainActor
final class ReadoutFeature: Feature {
    unowned let host: ViewerModel

    init(host: ViewerModel) { self.host = host }

    var inspectorSections: [InspectorSection] {
        [InspectorSection("readout", slot: .top) {
            if let controller = self.host.controller {
                ReadoutSections(controller: controller, session: self.host.session, build: self.host.wiring?.build,
                                extraLines: self.host.features.statusLines,
                                draw: self.host.features[DrawFeature.self]?.model.readoutState,
                                statsRows: self.host.features.inspectorSections(.stats),
                                plateFit: { self.host.features[PlatesFeature.self]?.board.fit ?? PlateFit() })
            }
        }]
    }
}
