import SwiftUI

/// The tip cards of the Plate ties section.
enum PlateTiesTip {
    static var profile: String { JS.t("The printer you print on. It sets the plate ties' defaults: the thinnest tie follows from its pixel size.") }
    static var on: String { JS.t("Tall, thin models side by side hold each other up with thin breakaway ties. While on, Arrange and Move are locked, and the plate exports as one object.") }
    static var maxGap: String { JS.t("Two models are neighbours when the gap between them is at most this.") }
    static var minRatio: String { JS.t("Only tall, thin models are tied: height at least this many times their thickness.") }
    static var minHeight: String { JS.t("Models shorter than this are not tied.") }
    static var contact: String { JS.t("Where a tie meets a model. Edge band: a band just inside the outline, easy to clean up; where it has no room the tie goes on the face, and the status says so. Facing faces: the middle of the faces that look at each other. Anywhere: the shortest crossing.") }
    static var bandWidth: String { JS.t("How far in from the outline the edge band reaches.") }
    static var ties: String { JS.t("How many ties join each pair, spread over the height range.") }
    static var contactThickness: String { JS.t("How thick a tie is where it meets a model: thinner snaps off cleaner. Never below what the printer can print.") }
    static var middleThickness: String { JS.t("How thick a tie is in the middle, between the models: thicker is stiffer.") }
    static var heightRange: String { JS.t("Where the ties go, as a share of the height the two models have in common. None go right by the bed.") }
    static var links: String { JS.t("The pairs found on this plate. Turn one off to leave that pair untied; a click on its link in the view does the same.") }
    static var addPair: String { JS.t("Tie two models the detection left apart: click this, then the two models.") }
    static var restoreLinks: String { JS.t("Bring back every link as the detection finds it: the ones turned off, and none added by hand.") }
    static var reset: String { JS.t("Put every plate tie setting back to the printer profile's defaults.") }
}

/// "Plate ties" (issue #50, Q9/Q10): the plate's ties on or off and their settings, after the settings block. Every
/// change goes through the engine (`plate_ties_settings`), which refuses what cannot print; the refusal shows under
/// the fields and the edit is not applied.
struct PlateTiesSection: View {
    let model: PlateTiesModel
    @Environment(SettingsSearch.self) private var search: SettingsSearch?
    @SceneStorage("sf.plateTiesOpen") private var open = false

    var body: some View {
        FindBlock(.plateTies) {
            Section {
                DisclosureGroup(isExpanded: search?.expandedBinding(.plateTies, user: $open) ?? $open) {
                    if model.state.on, let s = model.settings { fields(s) }
                    if let e = model.settingsError {
                        Text(verbatim: e).font(.caption).foregroundStyle(.red).accessibilityIdentifier("plate-ties-error")
                    }
                } label: {
                    FindRow("plate-ties-toggle", hides: false) {
                        Toggle(isOn: Binding(get: { model.state.on }, set: { on in
                            model.setOn(on)
                            if on && model.state.on { open = true }        // its settings and links in view
                        })) {
                            Text("Plate ties")
                        }
                        .accessibilityIdentifier("plate-ties-toggle")
                        .tipCard(PlateTiesTip.on)
                    }
                }
            }
        }
    }

    private func set(_ edit: (inout TieSettings) -> Void, _ s: TieSettings) {
        var n = s
        edit(&n)
        if n != s { model.update(n) }
    }

    private func number(_ s: TieSettings, _ key: WritableKeyPath<TieSettings, Double>, scale: Double = 1) -> Binding<Double> {
        Binding(get: { s[keyPath: key] * scale }, set: { v in set({ $0[keyPath: key] = v / scale }, s) })
    }

    @ViewBuilder private func fields(_ s: TieSettings) -> some View {
        FindRow("plate-ties-max-gap") {
            NumberField(title: "Max gap", value: number(s, \.maxGap), limits: TieSettings.maxGapLimits, unit: "mm", help: PlateTiesTip.maxGap, fieldID: "plate-ties-max-gap")
        }
        FindRow("plate-ties-min-ratio") {
            NumberField(title: "Min height / thickness", value: number(s, \.minRatio), limits: TieSettings.minRatioLimits, unit: "×", help: PlateTiesTip.minRatio)
        }
        FindRow("plate-ties-min-height") {
            NumberField(title: "Min height", value: number(s, \.minHeight), limits: TieSettings.minHeightLimits, unit: "mm", help: PlateTiesTip.minHeight)
        }
        FindRow("plate-ties-contact") {
            Picker("Contact", selection: Binding(get: { s.contact }, set: { c in set({ $0.contact = c }, s) })) {
                Text("Edge band").tag(TieSettings.Contact.edge)
                Text("Facing faces").tag(TieSettings.Contact.face)
                Text("Anywhere").tag(TieSettings.Contact.anywhere)
            }
            .tipCard(PlateTiesTip.contact)
        }
        if s.contact == .edge {
            FindRow("plate-ties-band") {
                NumberField(title: "Band width", value: number(s, \.bandWidth), limits: TieSettings.bandWidthLimits, unit: "mm", help: PlateTiesTip.bandWidth)
            }
        }
        FindRow("plate-ties-count") {
            LabeledContent {
                HStack(spacing: 4) {
                    Text(verbatim: "\(s.tiesPerPair)").monospacedDigit()
                    Stepper("Ties per pair", value: Binding(get: { s.tiesPerPair }, set: { n in set({ $0.tiesPerPair = n }, s) }), in: TieSettings.tiesLimits)
                        .labelsHidden()
                }
            } label: { Text("Ties per pair") }
            .tipCard(PlateTiesTip.ties)
        }
        FindRow("plate-ties-contact-thickness") {
            NumberField(title: "Contact thickness", value: number(s, \.contactThickness), limits: TieSettings.contactLimits, unit: "mm", help: PlateTiesTip.contactThickness)
        }
        FindRow("plate-ties-middle-thickness") {
            NumberField(title: "Middle thickness", value: number(s, \.middleThickness), limits: TieSettings.middleLimits, unit: "mm", help: PlateTiesTip.middleThickness)
        }
        FindRow("plate-ties-height") {
            NumberField(title: "Ties from", value: number(s, \.heightFrom, scale: 100), limits: TieSettings.percentLimits, unit: "% of height", help: PlateTiesTip.heightRange)
            NumberField(title: "Ties up to", value: number(s, \.heightTo, scale: 100), limits: TieSettings.percentLimits, unit: "% of height", help: PlateTiesTip.heightRange)
        }
        // every link found, a switch each: the same as a click on it in the view (and the way to it without a pointer)
        if !model.drawn.isEmpty {
            FindRow("plate-ties-links") {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Links").foregroundStyle(.secondary)
                    ForEach(Array(model.drawn.enumerated()), id: \.offset) { k, l in
                        Toggle(isOn: Binding(get: { model.isTied(l) }, set: { _ in model.toggle(l) })) {
                            Text(verbatim: "\(model.name(l.a)) – \(model.name(l.b))").lineLimit(1).truncationMode(.middle)
                        }
                        .accessibilityIdentifier("plate-tie-link-\(k)")
                    }
                }
                .tipCard(PlateTiesTip.links)
            }
        }
        FindRow("plate-ties-add-pair") {
            HStack {
                Button { model.setPairing(model.pairing == nil) } label: {
                    if model.pairing == nil { Text("Add a pair") } else { Text("Cancel pair") }
                }
                    .accessibilityIdentifier("plate-ties-add-pair")
                    .tipCard(PlateTiesTip.addPair)
                if model.hasOverrides {
                    Button("Restore links") { model.restoreLinks() }
                        .accessibilityIdentifier("plate-ties-restore-links")
                        .tipCard(PlateTiesTip.restoreLinks)
                }
            }
        }
        FindRow("plate-ties-reset") {
            Button("Reset to profile defaults") { model.resetToProfile() }
                .accessibilityIdentifier("plate-ties-reset")
                .tipCard(PlateTiesTip.reset)
        }
    }
}
