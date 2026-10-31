import Observation
import SupportFinsCore
import SwiftUI

/// The printer the plate ties are made for (issue #50, Q8): one of the engine's printer profiles (`printer_profiles()`,
/// the Elegoo Saturn 4 Ultra 16K by default), chosen in the inspector's Printer section and remembered across launches
/// (`sf.printerProfile`). A profile sets the ties' defaults; the thinnest tie follows from its pixel size.
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
}

/// The Printer section's first row: which printer the plate ties are made for.
struct PrinterProfilePicker: View {
    @Bindable var store: PrinterProfileStore = .shared

    var body: some View {
        FindRow("printer-profile") {
            Picker("Printer", selection: $store.id) {
                ForEach(store.profiles, id: \.id) { p in Text(verbatim: p.name).tag(p.id) }
            }
            .accessibilityIdentifier("printer-profile")
            .tipCard(PlateTiesTip.profile)
        }
    }
}
