import Foundation

/// The settings the web remembers across visits in `localStorage` (`docs/UI-MAP.md` section 9), kept in
/// `UserDefaults` under the same `sf.*` names. The other fin settings are per part: M7 stores them in the
/// document's session.
struct SettingsStore {
    var defaults: UserDefaults = .standard

    enum Key {
        static let nozzle = "sf.nozzle"                  // web/ui/settings.js:297-306
        static let layerHeight = "sf.layerHeight"        // settings.js:315-324
        static let gap = "sf.gap"
        static let highlightSmall = "sf.highlightSmall"  // settings.js:386-392
        static let showIslands = "sf.showIslands"        // web/ui/islands.js:28-33
        static let sections = "sf.sections"              // settings.js:363-375
    }

    /// `base` with the remembered values laid over it. A stored number outside its field's range, or a nozzle
    /// that is not on the menu, is ignored, as the web ignores it.
    func load(into base: FinSettings = FinSettings()) -> FinSettings {
        var s = base
        if let d = number(Key.nozzle), FinSettings.nozzleChoices.contains(d) {
            s.setNozzle(d)
        }
        if let d = number(Key.layerHeight), FinSettings.layerHeightLimits.range.contains(d) { s.layerHeight = d }
        if let d = number(Key.gap), FinSettings.gapLimits.range.contains(d) { s.gap = d }
        if defaults.object(forKey: Key.highlightSmall) != nil { s.highlightSmall = defaults.bool(forKey: Key.highlightSmall) }
        if defaults.object(forKey: Key.showIslands) != nil { s.showIslands = defaults.bool(forKey: Key.showIslands) }
        return s
    }

    /// Remember the values the web remembers.
    func save(_ s: FinSettings) {
        defaults.set(s.nozzle, forKey: Key.nozzle)
        defaults.set(s.layerHeight, forKey: Key.layerHeight)
        defaults.set(s.gap, forKey: Key.gap)
        defaults.set(s.highlightSmall, forKey: Key.highlightSmall)
        defaults.set(s.showIslands, forKey: Key.showIslands)
    }

    private func number(_ key: String) -> Double? {
        guard defaults.object(forKey: key) != nil else { return nil }
        let d = defaults.double(forKey: key)
        return d.isFinite ? d : nil
    }
}

/// Which inspector sections are open (`sf.sections`, `web/ui/settings.js:363-380`): `{printer, tines, more}`.
/// The web starts with Tines open and the others shut.
struct SectionState: Codable, Equatable, RawRepresentable {
    var printer = false
    var tines = true
    var more = false

    init() {}

    // Spelled out: a RawRepresentable type's synthesized Codable would encode its rawValue, which encodes it.
    private enum CodingKeys: String, CodingKey { case printer, tines, more }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        printer = try c.decodeIfPresent(Bool.self, forKey: .printer) ?? false
        tines = try c.decodeIfPresent(Bool.self, forKey: .tines) ?? true
        more = try c.decodeIfPresent(Bool.self, forKey: .more) ?? false
    }
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(printer, forKey: .printer)
        try c.encode(tines, forKey: .tines)
        try c.encode(more, forKey: .more)
    }

    /// A `@SceneStorage` value: the JSON object, or the defaults for anything unreadable.
    init?(rawValue: String) {
        guard let data = rawValue.data(using: .utf8), let s = try? JSONDecoder().decode(SectionState.self, from: data) else {
            if rawValue.isEmpty { self.init(); return }
            return nil
        }
        self = s
    }

    var rawValue: String {
        let enc = JSONEncoder()
        enc.outputFormatting = [.sortedKeys]
        return (try? String(data: enc.encode(self), encoding: .utf8)) ?? ""
    }
}
