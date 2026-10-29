import SwiftUI

/// More settings: four groups, "N changed" in the header and a dot on each changed field
/// (`index.html:463-593`, `web/ui/more.js`).
struct MoreSettingsSection: View {
    @Environment(PartSession.self) private var session
    @Binding var settings: FinSettings
    @Binding var open: Bool
    @Environment(SettingsSearch.self) private var search: SettingsSearch?

    var body: some View {
        FindBlock(.more) { group }
    }

    private var group: some View {
        let v = settings.visibility, changed = settings.changedFields
        return DisclosureGroup(isExpanded: search?.expandedBinding(.more, user: $open) ?? $open) {
            FindBlock(.holdThePart) {
                Section("Hold the part") {
                    if v.roundedFeet {
                        FindRow("rounded-feet-fld") { ChangedToggle(title: "Rounded feet", isOn: $settings.roundedFeet, changed: changed.contains(.roundedFeet), help: Tip.roundedFeet, pic: "rounded-feet") }
                    }
                    FindRow("bed-pad") {
                        Picker(selection: $settings.bedPad) {
                            Text("Off").tag(FinSettings.BedPad.off)
                            Text("Auto").tag(FinSettings.BedPad.auto)
                            Text("Light").tag(FinSettings.BedPad.light)
                            Text("Sure hold").tag(FinSettings.BedPad.sure)
                            Text("Custom").tag(FinSettings.BedPad.custom)
                        } label: { ChangedLabel("Bed pad", changed: changed.contains(.bedPad)) }
                        .tipCard(pic: "bed-pad", Tip.bedPad)
                    }
                    if v.padCustom {
                        FindRow("pad-h") { NumberField(title: "Pad thickness", value: $settings.padThickness, limits: FinSettings.padThicknessLimits, unit: "mm", changed: changed.contains(.padThickness), help: Tip.padH, pic: "pad-h") }
                        FindRow("pad-gap") { NumberField(title: "Pad gap", value: $settings.padGap, limits: FinSettings.padGapLimits, unit: "mm", changed: changed.contains(.padGap), help: Tip.padGap, pic: "pad-gap") }
                        FindRow("pad-grip") { NumberField(title: "Pad grip", value: $settings.padGrip, limits: FinSettings.padGripLimits, unit: "mm", changed: changed.contains(.padGrip), help: Tip.padGrip, pic: "pad-grip") }
                        FindRow("pad-margin") { NumberField(title: "Pad spread", value: $settings.padSpread, limits: FinSettings.padSpreadLimits, unit: "mm", changed: changed.contains(.padSpread), help: Tip.padMargin, pic: "pad-margin") }
                    }
                    FindRow("wall-lines") {
                        Picker(selection: $settings.wallLines) {
                            ForEach(FinSettings.wallLineChoices, id: \.self) { n in
                                Text("\(n)").tag(n).disabled(!FinSettings.wallLinesFit(n, nozzle: settings.nozzle))
                            }
                        } label: { ChangedLabel("Wall lines", changed: changed.contains(.wallLines)) }
                        .tipCard(pic: "wall-lines", Tip.wallLines)
                    }
                }
            }
            FindBlock(.tallParts) {
                Section("Tall parts") {
                    FindRow("tall-ribs") { ChangedToggle(title: "Brace tall walls", isOn: $settings.tallRibs, changed: changed.contains(.tallRibs), help: Tip.tallRibs, pic: "tall-ribs") }
                    if v.tallRibsHeight {
                        FindRow("tall-ribs-height-fld") { NumberField(title: "Brace walls above", value: $settings.tallRibsHeight, limits: FinSettings.tallRibsHeightLimits, unit: "cm", changed: changed.contains(.tallRibsHeight), help: Tip.tallRibsHeight, pic: "tall-ribs-height") }
                    }
                    FindRow("sway") { ChangedToggle(title: "Sway braces", isOn: $settings.sway, changed: changed.contains(.sway), help: Tip.sway, pic: "sway") }
                    if v.swayFrom {
                        FindRow("sway-from-fld") { NumberField(title: "Brace grip from", value: $settings.swayFrom, limits: FinSettings.swayFromLimits, unit: "mm up", changed: changed.contains(.swayFrom), help: Tip.swayFrom, pic: "sway-from") }
                    }
                    if v.swaySpacing {
                        FindRow("sway-spacing-fld") { NumberField(title: "Brace tine spacing", value: $settings.swaySpacing, limits: FinSettings.swaySpacingLimits, unit: "mm", changed: changed.contains(.swaySpacing), help: Tip.swaySpacing, pic: "sway-spacing") }
                    }
                    if v.swayDepth {
                        FindRow("sway-depth-fld") { NumberField(title: "Brace depth", value: $settings.swayDepth, limits: FinSettings.swayDepthLimits, unit: "% of height", changed: changed.contains(.swayDepth), help: Tip.swayDepth, pic: "sway-depth") }
                    }
                    if v.fullBraceDepth {
                        FindRow("full-brace-depth-fld") { ChangedToggle(title: "Full brace depth", isOn: $settings.fullBraceDepth, changed: changed.contains(.fullBraceDepth), help: Tip.fullBraceDepth, pic: "full-brace-depth") }
                    }
                    if v.swayCrossHeight {
                        FindRow("sway-cross-height-fld") { NumberField(title: "Cross-rib height", value: $settings.swayCrossHeight, limits: FinSettings.swayCrossHeightLimits, unit: "% of height", changed: changed.contains(.swayCrossHeight), help: Tip.swayCrossHeight, pic: "sway-cross-height") }
                    }
                }
            }
            FindBlock(.whereSupportsGo) {
                Section("Where supports go") {
                    if v.coverage {
                        FindRow("coverage-fld") {
                            LabeledContent {
                                HStack {
                                    Text("sparse").font(.caption).foregroundStyle(.secondary)
                                    Slider(value: $settings.coverage, in: 0...100, step: 5, onEditingChanged: session.settingsEditing)
                                    .accessibilityLabel("Wide-face coverage")
                                    .accessibilityValue(JS.t("{n} percent", ["n": String(Int(settings.coverage))]))
                                    Text("dense").font(.caption).foregroundStyle(.secondary)
                                }
                            } label: { ChangedLabel("Wide-face coverage", changed: changed.contains(.coverage)) }
                            .disabled(v.coverageDisabled)
                            .tipCard(pic: "coverage", v.coverageDisabled ? Self.coverageTipFull : Tip.coverage)
                        }
                    }
                    FindRow("plate-only") { ChangedToggle(title: "Plate only", isOn: $settings.plateOnly, changed: changed.contains(.plateOnly), help: Tip.plateOnly, pic: "plate-only") }
                    if v.separateWalls {
                        FindRow("separate-walls-fld") { ChangedToggle(title: "Separate walls", isOn: $settings.separateWalls, changed: changed.contains(.separateWalls), help: Tip.separateWalls, pic: "separate-walls") }
                    }
                    if v.longWalls {
                        FindRow("long-walls-fld") { ChangedToggle(title: "Long walls", detail: "along the part", isOn: $settings.longWalls, changed: changed.contains(.longWalls), help: Tip.longWalls, pic: "long-walls") }
                    }
                    if v.curvedWalls {
                        FindRow("curved-walls-fld") { ChangedToggle(title: "Curved walls", detail: "follow the curve, more plastic", isOn: $settings.curvedWalls, changed: changed.contains(.curvedWalls), help: Tip.curvedWalls, pic: "curved-walls") }
                    }
                }
            }
            FindBlock(.display) {
                Section("Display") {
                    FindRow("highlight-small") { ChangedToggle(title: "Highlight small overhangs", isOn: $settings.highlightSmall, changed: changed.contains(.highlightSmall), help: Tip.highlightSmall, pic: "highlight-small") }
                    FindRow("show-islands") { ChangedToggle(title: "Show midair starts", isOn: $settings.showIslands, changed: changed.contains(.showIslands), help: Tip.showIslands, pic: "show-islands") }
                }
            }
        } label: {
            HStack {
                Text("More settings")
                Spacer()
                Text(settings.changedSummary).font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    /// The coverage slider's tooltip in Full coverage (`COVER_TIP_FULL`, `web/ui/settings.js:50-51`).
    static var coverageTipFull: String { JS.t("Full coverage supports every overhang a wall can reach, so this slider only applies in Auto.") }
}
