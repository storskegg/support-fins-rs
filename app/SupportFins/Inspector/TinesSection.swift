import SwiftUI

/// Tines: the section header is the switch; grip, direction, pin depth and the resin note follow it
/// (`index.html:358-405`, `web/ui/settings.js:172-195`).
struct TinesSection: View {
    @Environment(PartSession.self) private var session
    @Binding var settings: FinSettings
    @Binding var open: Bool
    @Environment(SettingsSearch.self) private var search: SettingsSearch?

    static var resinNote: String { JS.t("Not tested on a resin printer yet. Tell us how it prints on GitHub issue #309.") }

    var body: some View {
        FindBlock(.tines) { group }
    }

    private var group: some View {
        let v = settings.visibility
        return DisclosureGroup(isExpanded: search?.expandedBinding(.tines, user: $open) ?? $open) {
            if v.tineGrip {
                // "Tine grip light - firm"; for Vertical it spaces pins: "Pins per wall fewer - more"
                // (`index.html:380`, `settings.js:172-195`).
                FindRow("tinegrip-fld") {
                    LabeledContent(settings.pins ? "Pins per wall" : "Tine grip") {
                        HStack {
                            Text(settings.pins ? "fewer" : "light").font(.caption).foregroundStyle(.secondary)
                            Slider(value: $settings.tineDensity, in: 0...100, step: 5, onEditingChanged: session.settingsEditing)
                                .accessibilityIdentifier("tine-density")
                                .accessibilityLabel(settings.pins ? "Pins per wall" : "Tine grip")
                                .accessibilityValue(JS.t("{n} percent", ["n": String(Int(settings.tineDensity))]))
                            Text(settings.pins ? "more" : "firm").font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    .tipCard(pic: settings.pins ? "pin-spacing" : "tine-density", settings.pins ? Self.pinSpacingTip : Tip.tineDensity)
                }
            }
            if v.tineDirection {
                FindRow("tinedir-fld") {
                    Picker("Direction", selection: Binding(get: { settings.tineDirection }, set: { settings.chooseTineDirection($0) })) {
                        Text("Horizontal (FDM)").tag(FinSettings.TineDirection.horizontal)
                        Text("Vertical (resin)").tag(FinSettings.TineDirection.vertical)
                    }
                    .tipCard(pic: "tine-dir", Tip.tineDir)
                }
            }
            if v.pinDepth {
                FindRow("pindepth-fld") {
                    NumberField(title: "Pin depth", value: $settings.pinDepth, limits: FinSettings.pinDepthLimits, help: Tip.pinDepth, pic: "pin-depth")
                }
            }
            if v.resinNote {
                Text(Self.resinNote).font(.caption).foregroundStyle(.secondary)
            }
        } label: {
            HStack {
                FindRow("tines", hides: false) { Toggle(isOn: $settings.tines) { ChangedLabel("Tines", detail: "grip the part", changed: false) }.tipCard(pic: "tines", Tip.tines) }
                Spacer()
                if !open { Text(settings.tinesSummary).font(.caption).foregroundStyle(.secondary) }
            }
        }
    }

    /// The slider's tooltip for Vertical tines (`PIN_SPACING_TIP`, `web/ui/settings.js:176-178`).
    nonisolated static var pinSpacingTip: String { JS.t("How many pins each wall gets. More pins = a firmer hold; fewer = fewer marks to clean up. Add pins before deepening them: a deeper pin leaves a bigger mark and is harder to snap off.") }
}
