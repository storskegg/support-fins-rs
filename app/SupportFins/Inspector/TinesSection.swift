import SwiftUI

/// Tines: the section header is the switch; grip, direction, pin depth and the resin note follow it
/// (`index.html:358-405`, `web/ui/settings.js:172-195`).
struct TinesSection: View {
    @Environment(PartSession.self) private var session
    @Binding var settings: FinSettings
    @Binding var open: Bool

    static let resinNote = "Not tested on a resin printer yet. Tell us how it prints on GitHub issue #309."

    var body: some View {
        let v = settings.visibility
        DisclosureGroup(isExpanded: $open) {
            if v.tineGrip {
                // "Tine grip light - firm"; for Vertical it spaces pins: "Pins per wall fewer - more"
                // (`index.html:380`, `settings.js:172-195`).
                LabeledContent(settings.pins ? "Pins per wall" : "Tine grip") {
                    HStack {
                        Text(settings.pins ? "fewer" : "light").font(.caption).foregroundStyle(.secondary)
                        Slider(value: $settings.tineDensity, in: 0...100, step: 5, onEditingChanged: session.settingsEditing)
                        Text(settings.pins ? "more" : "firm").font(.caption).foregroundStyle(.secondary)
                    }
                }
                .help(settings.pins ? Self.pinSpacingTip : Tip.tineDensity)
            }
            if v.tineDirection {
                Picker("Direction", selection: Binding(get: { settings.tineDirection }, set: { settings.chooseTineDirection($0) })) {
                    Text("Horizontal (FDM)").tag(FinSettings.TineDirection.horizontal)
                    Text("Vertical (resin)").tag(FinSettings.TineDirection.vertical)
                }
                .help(Tip.tineDir)
            }
            if v.pinDepth {
                NumberField(title: "Pin depth", value: $settings.pinDepth, limits: FinSettings.pinDepthLimits, help: Tip.pinDepth)
            }
            if v.resinNote {
                Text(Self.resinNote).font(.caption).foregroundStyle(.secondary)
            }
        } label: {
            HStack {
                Toggle("Tines - grip the part", isOn: $settings.tines).help(Tip.tines)
                Spacer()
                if !open { Text(settings.tinesSummary).font(.caption).foregroundStyle(.secondary) }
            }
        }
    }

    /// The slider's tooltip for Vertical tines (`PIN_SPACING_TIP`, `web/ui/settings.js:176-178`).
    static let pinSpacingTip = "How many pins each wall gets. More pins = a firmer hold; fewer = fewer marks to clean up. Add pins before deepening them: a deeper pin leaves a bigger mark and is harder to snap off."
}
