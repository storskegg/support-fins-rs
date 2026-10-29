import SwiftUI
import Viewer

/// The inspector, in the web's section order (docs/UI-MAP.md section 4): Printer, Placement, Tines, the
/// basics, More settings. The settings are the session's (`PartSession.settings`): a change is one undoable step.
/// The Part block is M4a's, kept until the stats card lands.
struct InspectorView: View {
    @Bindable var controller: ViewerController
    @Binding var settings: FinSettings
    /// The features' sections (`Feature.inspectorSections`) at the top, in the settings block after Cutouts, and
    /// after it. An empty top keeps M4a's Part block in place of the stats and status cards.
    var top: [InspectorSection] = []
    var basics: [InspectorSection] = []
    var bottom: [InspectorSection] = []
    @SceneStorage(SettingsStore.Key.sections) private var sections = SectionState()

    var body: some View {
        Form {
            ForEach(top) { $0.content() }
            PartSections(controller: controller, withPartBlock: top.isEmpty)
            Section {
                PrinterSection(settings: $settings, open: $sections.printer)
                PlacementSection(settings: $settings)
                TinesSection(settings: $settings, open: $sections.tines)
                Picker("Cutouts", selection: $settings.cutout) {
                    Text("None").tag(FinSettings.Cutout.none)
                    Text("Diamond").tag(FinSettings.Cutout.diamond)
                    Text("Triangle").tag(FinSettings.Cutout.triangle)
                    Text("Arch").tag(FinSettings.Cutout.arch)
                    Text("Lattice").tag(FinSettings.Cutout.lattice)
                    Text("Honeycomb").tag(FinSettings.Cutout.honeycomb)
                }
                .help(Tip.cutout)
                ForEach(basics) { $0.content() }
                MoreSettingsSection(settings: $settings, open: $sections.more)
            }
            ForEach(bottom) { $0.content() }
            #if DEBUG
            // the last row of the form, not a bar over it: a `safeAreaInset` bar had no background, so the rows
            // scrolled under it and a click aimed at the one beneath (the Tine grip slider) landed on the bar
            Section {
                Toggle("Benchmark orbit", isOn: Binding(get: { controller.benchmarking }, set: { controller.setBenchmark($0) }))
            }
            #endif
        }
        .formStyle(.grouped)
        .onChange(of: settings) { _, new in SettingsStore().save(new) }
        // Highlight small overhangs paints the part, never rebuilds the fins (`web/ui/settings.js:386-392`)
        .onChange(of: settings.highlightSmall, initial: true) { _, on in controller.highlightSmall = on }
        // Show midair starts draws the red cones only (`web/ui/islands.js:30-34`)
        .onChange(of: settings.showIslands, initial: true) { _, on in controller.showsIslands = on }
    }
}
