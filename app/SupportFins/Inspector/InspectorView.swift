import SwiftUI
import Viewer

/// The inspector, in the web's section order (docs/UI-MAP.md section 4): Printer, Placement, Tines, the
/// basics, More settings. The Part block and the temporary rotation are M4a's, kept until the session owns
/// the pose.
struct InspectorView: View {
    @Bindable var controller: ViewerController
    @Binding var settings: FinSettings
    @SceneStorage(SettingsStore.Key.sections) private var sections = SectionState()
    @State private var customVolume = false

    var body: some View {
        Form {
            PartSections(controller: controller)
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
                MoreSettingsSection(settings: $settings, open: $sections.more)
            }
        }
        .formStyle(.grouped)
        .onChange(of: settings) { _, new in SettingsStore().save(new) }
        // Highlight small overhangs paints the part, never rebuilds the fins (`web/ui/settings.js:386-392`)
        .onChange(of: settings.highlightSmall, initial: true) { _, on in controller.highlightSmall = on }
        #if DEBUG
        .safeAreaInset(edge: .bottom) {
            Toggle("Benchmark orbit", isOn: Binding(get: { controller.benchmarking }, set: { controller.setBenchmark($0) }))
                .padding(8)
        }
        #endif
    }
}
