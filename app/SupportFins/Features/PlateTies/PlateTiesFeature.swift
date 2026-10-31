import SupportFinsCore
import SwiftUI
import Viewer

/// Plate ties (issue #50, slice 2): tall, thin models standing side by side on a plate hold each other up with thin
/// breakaway ties. The "Plate ties" section of the inspector turns them on and sets them; the links show on the
/// plate (a click turns one off or on, Add a pair ties two models clicked); the ties are drawn like the fins; the
/// status card says what could not be placed. While they are on, Arrange and Move are locked, and the plate's 3MF is
/// one merged object (`PlateBoard+Export`). The engine is `TiePlate` (docs/FFI.md "Plate ties and printer profiles");
/// the state is `PlateTiesModel`.
@MainActor
final class PlateTiesFeature: Feature {
    weak let host: ViewerModel?
    let model: PlateTiesModel
    /// The 3MF whose saved ties were last looked for: a part shown again (Scale) does not bring them back over edits.
    private var lookedAt: URL?

    init(host: ViewerModel) {
        self.host = host
        model = PlateTiesModel(host: host)
    }

    func partWillShow() { model.wire() }

    func partDidShow(_ parts: [LoadedPart]) {
        model.wire()
        model.hookExporter()
        guard let host, let board = host.features[PlatesFeature.self]?.board, !board.swapping else { return }
        // a tied 3MF opened onto a plate of its own: its ties come back (`Part.saved_plate_ties`, read with the file)
        if let url = host.openedURL, url != lookedAt, board.modelCount == 1, let doc = host.session.parts.first?.part.savedPlateTies() {
            model.apply(document: doc)
        }
        lookedAt = host.openedURL
    }

    func close() { model.close() }

    var modifiers: [WindowModifier] {
        [.onChange("plate-ties-profile", of: { PrinterProfileStore.shared.id }) { _, _ in self.model.profileChanged() },
         .onChange("plate-ties-profile-pick", of: { PrinterProfileStore.shared.picks }) { _, _ in
             if let p = PrinterProfileStore.shared.current { self.useProfile(p) }
         }]
    }

    /// A printer picked (decision A of #50): this window's build volume becomes the printer's (`sf.volume`, the plate
    /// and every export's bed) and its layer height the printer's usual layer (one undo step, remembered as
    /// `sf.layerHeight`). Either can be changed by hand afterwards; the picker keeps showing the printer.
    func useProfile(_ p: PrinterProfileInfo, volumes: VolumeStore = VolumeStore(), store: SettingsStore = SettingsStore()) {
        guard let host else { return }
        if let v = p.volume {
            host.controller?.volume = v
            volumes.save(v)
        }
        var s = host.session.settings
        s.layerHeight = p.layerHeight
        host.session.setSettings(s)
        store.save(host.session.settings)
    }

    var inspectorSections: [InspectorSection] {
        [InspectorSection("plate-ties", slot: .bottom) {
            if self.host?.hasPart == true { PlateTiesSection(model: self.model) }
        }]
    }

    var escapeHandlers: [EscapeHandler] {
        [EscapeHandler(id: "plate-ties-pair", priority: EscapeHandler.pointerMode) {
            guard self.model.pairing != nil else { return false }
            self.model.setPairing(false)
            return true
        }]
    }

    /// The status card: the lock and why, what the ties came to, and every note of the build.
    var statusLines: [StatusLine] {
        let m = model
        guard m.state.on, host?.hasPart == true else { return [] }
        var lines: [StatusLine] = []
        if let why = m.lockReason { lines.append(StatusLine("plate-ties-lock", order: 40, text: why)) }
        if m.pairing != nil {
            lines.append(StatusLine("plate-ties-pairing", order: 41, text: JS.t("Click two models to tie them together. Esc to stop."), color: .accentColor))
        }
        if let e = m.error {
            lines.append(StatusLine("plate-ties-error", order: 42, text: JS.t("Plate ties: {why}", ["why": e]), color: .red, bar: .red))
        } else if m.working {
            lines.append(StatusLine("plate-ties-working", order: 42, text: JS.t("Building plate ties…")))
        } else if m.links.isEmpty, !m.candidates.isEmpty {
            lines.append(StatusLine("plate-ties-none", order: 42, text: JS.t("Plate ties: every link on this plate is turned off.")))
        } else if m.links.isEmpty {
            lines.append(StatusLine("plate-ties-none", order: 42, text: JS.t("Plate ties: no neighbours to tie on this plate.")))
        } else {
            lines.append(StatusLine("plate-ties-summary", order: 42, text: JS.t("Plate ties: {placed} of {requested} ties on {pairs} pairs",
                                                                              ["placed": String(m.placed), "requested": String(m.requested), "pairs": String(m.links.count)])))
        }
        let notes = m.working ? [] : m.notes
        let shown = 6
        for (k, n) in notes.prefix(shown).enumerated() {
            lines.append(StatusLine("plate-ties-note-\(k)", order: 43, text: n, color: .orange, bar: .orange))
        }
        if notes.count > shown {
            lines.append(StatusLine("plate-ties-more", order: 43, text: JS.t("…and {more} more", ["more": String(notes.count - shown)]), color: .orange))
        }
        return lines
    }
}
