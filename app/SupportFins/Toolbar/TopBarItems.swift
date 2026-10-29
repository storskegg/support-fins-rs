import SwiftUI
import Viewer

/// The remembered build volume (`sf.volume`, `web/ui/volume.js:25-37`): the JSON object `{x, y, z}` in
/// `UserDefaults`. A stored volume with a side at zero or less is ignored, as the web ignores it.
struct VolumeStore {
    var defaults: UserDefaults = .standard
    static let key = "sf.volume"
    /// The custom fields' range (`index.html:98-99`, `ui/volume.js:30`).
    static let sideRange: ClosedRange<Double> = 20...2000

    func load() -> PrinterVolume {
        guard let text = defaults.string(forKey: Self.key), let v = try? JSONDecoder().decode(PrinterVolume.self, from: Data(text.utf8)),
              v.isValid else { return .standard }
        return v
    }

    func save(_ v: PrinterVolume) {
        guard let data = try? JSONEncoder().encode(v), let text = String(data: data, encoding: .utf8) else { return }
        defaults.set(text, forKey: Self.key)
    }

    /// A custom side as typed, held to the fields' 20...2000 mm.
    static func clamped(_ side: Double) -> Float { Float(min(max(side.rounded(), sideRange.lowerBound), sideRange.upperBound)) }
}

/// `#volume` (`index.html:95`): the presets and Custom… as a pop-up in the window toolbar. Custom… opens a sheet
/// with the width, depth and height fields (`vx`, `vy`, `vz`).
struct VolumePicker: View {
    @Bindable var controller: ViewerController
    var store = VolumeStore()
    @State private var customOpen = false

    var body: some View {
        Picker("Build volume", selection: Binding<PrinterVolume?>(
            get: { controller.volume },
            set: { if let v = $0 { choose(v) } else { customOpen = true } })) {
            ForEach(PrinterVolume.presets, id: \.self) { Text($0.label).tag(Optional($0)) }
            if !controller.volume.isPreset { Text(controller.volume.label).tag(Optional(controller.volume)) }
            Divider()
            Text("Custom…").tag(Optional<PrinterVolume>.none)
        }
        .help("Build volume")
        .accessibilityIdentifier("volume")
        .task { controller.volume = store.load() }
        .sheet(isPresented: $customOpen) { CustomVolumeSheet(start: controller.volume) { choose($0) } }
    }

    private func choose(_ v: PrinterVolume) {
        controller.volume = v
        store.save(v)
    }
}

/// `#custom-vol` (`index.html:96-100`): width × depth × height, 20..2000 mm.
struct CustomVolumeSheet: View {
    let start: PrinterVolume
    let apply: (PrinterVolume) -> Void
    @State private var x = 0.0
    @State private var y = 0.0
    @State private var z = 0.0
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Custom build volume").font(.headline)
            Grid(alignment: .leading, horizontalSpacing: 8, verticalSpacing: 8) {
                field("Width", $x, help: "width", id: "vx")
                field("Depth", $y, help: "depth", id: "vy")
                field("Height", $z, help: "height", id: "vz")
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Use This Size") {
                    apply(PrinterVolume(x: VolumeStore.clamped(x), y: VolumeStore.clamped(y), z: VolumeStore.clamped(z)))
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 280)
        .onAppear { x = Double(start.x); y = Double(start.y); z = Double(start.z) }
    }

    private func field(_ title: LocalizedStringKey, _ value: Binding<Double>, help: LocalizedStringKey, id: String) -> some View {
        GridRow {
            Text(title)
            HStack(spacing: 4) {
                TextField(title, value: value, format: .number.precision(.fractionLength(0)))
                    .multilineTextAlignment(.trailing).frame(width: 70).help(help).accessibilityIdentifier(id)
                Stepper(title, value: value, in: VolumeStore.sideRange, step: 1).labelsHidden()
                Text("mm").foregroundStyle(.secondary)
            }
        }
    }
}

/// `#thr` with its `#thr-val` (`index.html:104-105`, `ui/part.js:503`): the overhang threshold, 30..70°. The
/// threshold is a session setting, so a drag is one undo step (`PartSession.settingsEditing`).
struct OverhangSlider: View {
    let session: PartSession

    var body: some View {
        let threshold = Binding(get: { session.settings.threshold }, set: { v in
            var s = session.settings
            s.threshold = v
            session.setSettings(s)
        })
        HStack(spacing: 6) {
            Text("Overhang")
            Slider(value: threshold, in: 30...70, step: 1, onEditingChanged: session.settingsEditing) { Text("Overhang") }
                .frame(width: 110)
                .accessibilityIdentifier("thr")
                .accessibilityValue(JS.t("{n} degrees", ["n": String(Int(session.settings.threshold))]))
            Text("\(Int(session.settings.threshold))°").monospacedDigit().frame(minWidth: 32, alignment: .trailing)
        }
        .help("Surface angle from the plate below which a face needs support.")
    }
}
