import SwiftUI

/// Printer: the one-line summary with Edit / Done, unfolding to Nozzle, Layer height, Support gap and
/// Interface material (`index.html:289-326`, `docs/UI-MAP.md` 4.1).
struct PrinterSection: View {
    @Binding var settings: FinSettings
    @Binding var open: Bool

    var body: some View {
        DisclosureGroup(isExpanded: $open) {
            Picker("Nozzle", selection: Binding(get: { settings.nozzle }, set: { settings.setNozzle($0) })) {
                ForEach(FinSettings.nozzleChoices, id: \.self) { Text("\(FinSettings.nozzleText($0)) mm").tag($0) }
            }
            .tipCard(pic: "nozzle", Tip.nozzle)
            if settings.visibility.layerHeight {
                NumberField(title: "Layer height", value: $settings.layerHeight, limits: FinSettings.layerHeightLimits, unit: "mm", help: Tip.layerHeight, pic: "layer-height")
            }
            NumberField(title: "Support gap", value: $settings.gap, limits: FinSettings.gapLimits, unit: "mm", help: Tip.gap, pic: "gap")
            Picker("Interface material", selection: $settings.interfaceMaterial) {
                Text("Off").tag(FinSettings.InterfaceMaterial.off)
                Text("Flat contacts").tag(FinSettings.InterfaceMaterial.flat)
                Text("Everywhere").tag(FinSettings.InterfaceMaterial.all)
            }
            .tipCard(pic: "iface", Tip.iface)
        } label: {
            HStack {
                Image(systemName: "printer")
                Text(settings.printerSummary).lineLimit(1)
                Spacer()
                Text(open ? "Done" : "Edit").foregroundStyle(.secondary)
            }
            .contentShape(Rectangle())
            .onTapGesture { open.toggle() }
            .tipCard(pic: "printer", JS.t("Your printer: nozzle, layer height, support gap, interface material. Click to change."))
        }
    }
}
