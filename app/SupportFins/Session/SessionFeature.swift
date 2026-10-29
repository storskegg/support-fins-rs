import SwiftUI

/// Sessions (`web/ui/session.js`, `afterLoad` in `web/ui/io.js`): a 3MF this app or the site exported carries the
/// supports drawn by hand, the fin mode and the settings. When the file's part arrives they are restored, and the
/// import note in the status card says how many (or why none). Writing is `ExportModel` / `PlateBoard` (they ask
/// `PartSession.sessionJSON`); this feature is the opening half.
@MainActor
final class SessionFeature: Feature {
    unowned let host: ViewerModel

    init(host: ViewerModel) { self.host = host }

    func partDidShow(_ parts: [LoadedPart]) {
        guard let core = parts.count == 1 ? parts[0].mesh as? CorePart : nil, core.part.hasSession() else { return }
        // added to a plate of models: they keep the plate's settings (`beside`, `ui/models.js`); the board has already
        // given this model its slot
        let beside = (host.features[PlatesFeature.self]?.board.modelCount ?? 1) > 1
        let restored = core.part.restoreSession(beside: beside)
        core.refreshInfo()
        guard restored.why == nil else { return }
        SessionFile.apply(restored, to: host.session)
    }
}
