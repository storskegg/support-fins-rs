import SwiftUI

/// The inspector's readout: the stats and status cards on top, with the lines other features add to the status card.
@MainActor
final class ReadoutFeature: Feature {
    weak let host: ViewerModel?

    init(host: ViewerModel) { self.host = host }

    var inspectorSections: [InspectorSection] {
        [InspectorSection("readout", slot: .part) {
            if let host = self.host, let controller = host.controller {
                ReadoutSections(controller: controller, session: host.session, build: host.wiring?.build,
                                extraLines: host.features.statusLines,
                                draw: host.features[DrawFeature.self]?.model.readoutState,
                                statsRows: host.features.inspectorSections(.stats),
                                plateFit: { [weak host] in host?.features[PlatesFeature.self]?.board.fit ?? PlateFit() })
            }
        }]
    }

    /// A build that failed says so, in red, with the engine's words (S2: a failed build left Fins "—" and nothing else).
    var statusLines: [StatusLine] {
        guard let host else { return [] }
        return Self.buildFailure(finsOn: host.session.finsOn, building: host.wiring?.build?.building ?? false,
                                 error: host.wiring?.build?.error)
    }

    static func buildFailure(finsOn: Bool, building: Bool, error: String?) -> [StatusLine] {
        guard finsOn, !building, let error, !error.isEmpty else { return [] }
        let why = error.hasSuffix(".") ? error : error + "."
        return [StatusLine("build-error", order: 5,
                           text: JS.t("The supports could not be built: {why} Change a setting or the pose to try again.", ["why": why]),
                           color: .primary, bar: StatusCard.red)]
    }
}
