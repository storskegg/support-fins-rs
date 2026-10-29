import SwiftUI

/// The inspector's readout: the stats and status cards on top, with the lines other features add to the status card.
@MainActor
final class ReadoutFeature: Feature {
    weak let host: ViewerModel?

    init(host: ViewerModel) { self.host = host }

    var inspectorSections: [InspectorSection] {
        [InspectorSection("readout", slot: .top) {
            if let host = self.host, let controller = host.controller {
                ReadoutSections(controller: controller, session: host.session, build: host.wiring?.build,
                                extraLines: host.features.statusLines,
                                draw: host.features[DrawFeature.self]?.model.readoutState,
                                statsRows: host.features.inspectorSections(.stats),
                                plateFit: { [weak host] in host?.features[PlatesFeature.self]?.board.fit ?? PlateFit() })
            }
        }]
    }
}
