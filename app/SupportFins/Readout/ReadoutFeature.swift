import SwiftUI

/// The inspector's readout: the status card pinned above the tabs (with the lines other features add to it) and the
/// stats card in the Part tab. Both read one analysis of the part (`ReadoutState`).
@MainActor
final class ReadoutFeature: Feature {
    weak let host: ViewerModel?
    /// The part's analysis, run by the stats card's view and read by both cards.
    let state = ReadoutState()

    init(host: ViewerModel) { self.host = host }

    var inspectorSections: [InspectorSection] {
        // after the search box and its note (orders -20 and -10)
        [InspectorSection("status", slot: .header) {
            if let inputs = self.inputs() { PinnedStatus(inputs: inputs) }
         },
         InspectorSection("readout", slot: .part) {
            if let inputs = self.inputs(), let host = self.host {
                ReadoutSections(inputs: inputs, statsRows: host.features.inspectorSections(.stats))
            }
         }]
    }

    /// The window's readout inputs, read inside a section's content (so its view observes them); nil with no viewer.
    private func inputs() -> ReadoutInputs? {
        guard let host, let controller = host.controller else { return nil }
        return ReadoutInputs(controller: controller, session: host.session, build: host.wiring?.build,
                             extraLines: host.features.statusLines,
                             draw: host.features[DrawFeature.self]?.model.readoutState,
                             plateFit: { [weak host] in host?.features[PlatesFeature.self]?.board.fit ?? PlateFit() },
                             state: state)
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
