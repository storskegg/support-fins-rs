import Observation
import SupportFinsCore
import SwiftUI
import Viewer

/// The printer the plate ties are made for (issue #50, Q8): one of the engine's printer profiles (`printer_profiles()`,
/// the Elegoo Saturn 4 Ultra 16K by default), chosen in the inspector's Printer section and remembered across launches
/// (`sf.printerProfile`). A profile sets the ties' defaults; the thinnest tie follows from its pixel size. Picking one
/// (`choose`, even the one already shown) also sets every window's build volume and layer height to the printer's
/// (`PlateTiesFeature.useProfile`); a bed or layer changed by hand afterwards overrides them, the profile stays.
@MainActor
@Observable
final class PrinterProfileStore {
    static let key = "sf.printerProfile"
    /// The app's one store: the choice is the app's, not a window's.
    static let shared = PrinterProfileStore()

    /// Every profile the engine knows, the default first.
    let profiles: [PrinterProfileInfo]
    @ObservationIgnored private let defaults: UserDefaults

    /// The chosen profile's id: always one of `profiles` (an unknown stored id falls back to the default).
    var id: String {
        didSet {
            if !profiles.contains(where: { $0.id == id }) { id = Self.fallback(profiles) }
            defaults.set(id, forKey: Self.key)
        }
    }

    init(defaults: UserDefaults = .standard, profiles: [PrinterProfileInfo] = printerProfiles()) {
        self.defaults = defaults
        self.profiles = profiles
        let stored = defaults.string(forKey: Self.key)
        id = stored.flatMap { s in profiles.contains { $0.id == s } ? s : nil } ?? Self.fallback(profiles)
    }

    private static func fallback(_ profiles: [PrinterProfileInfo]) -> String {
        (profiles.first { $0.isDefault } ?? profiles.first)?.id ?? ""
    }

    var current: PrinterProfileInfo? { profiles.first { $0.id == id } }

    /// Every pick in the picker, the same profile again too: each window then takes the printer's bed and layer.
    private(set) var picks = 0

    /// The user picked `id` in the Printer section.
    func choose(_ id: String) {
        self.id = id
        picks += 1
    }
}

extension PrinterProfileInfo {
    /// The printer's build volume as the app's bed (`sf.volume`): x and y the plate, z the height; nil when the
    /// profile's is not three sides above zero.
    var volume: PrinterVolume? {
        guard buildVolume.count == 3 else { return nil }
        let v = PrinterVolume(x: Float(buildVolume[0]), y: Float(buildVolume[1]), z: Float(buildVolume[2]))
        return v.isValid ? v : nil
    }

    /// The printer's usual layer (`layerHeights.default`), held to the Layer height field's range.
    var layerHeight: Double {
        let r = FinSettings.layerHeightLimits.range
        return min(max(layerHeightDefault, r.lowerBound), r.upperBound)
    }
}

/// The Printer section's first row: which printer the plate ties are made for.
struct PrinterProfilePicker: View {
    var store: PrinterProfileStore = .shared

    var body: some View {
        FindRow("printer-profile") {
            // through `choose`: picking the printer shown again still sets the bed and layer
            Picker("Printer", selection: Binding(get: { store.id }, set: { store.choose($0) })) {
                ForEach(store.profiles, id: \.id) { p in Text(verbatim: p.name).tag(p.id) }
            }
            .accessibilityIdentifier("printer-profile")
            .tipCard(PlateTiesTip.profile)
        }
    }
}
