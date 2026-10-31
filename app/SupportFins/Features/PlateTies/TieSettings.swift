import Foundation
import SupportFinsCore

/// The plate-tie settings the inspector shows (issue #50, Q10), as the engine writes them
/// (`plate_ties_settings`, docs/FFI.md "Plate ties and printer profiles"): every key, the hidden tunables kept as they
/// come. The engine validates; the app only edits and round-trips.
struct TieSettings: Codable, Equatable, Sendable {
    enum Contact: String, Codable, CaseIterable, Sendable {
        case edge, face, anywhere
    }

    var contact: Contact
    var maxGap: Double
    var minRatio: Double
    var minHeight: Double
    var bandWidth: Double
    var tiesPerPair: Int
    var contactThickness: Double
    var middleThickness: Double
    var heightFrom: Double
    var heightTo: Double
    /// The hidden tunables (depth, arch, neck, ...): not shown, written back as read.
    var tunables: [String: Double]

    /// A field's range and step (`sf_engine::plateties::NUMBERS`; the steps are the inspector's).
    static let maxGapLimits = FinSettings.Limits(range: 0.5...50, step: 0.5)
    static let minRatioLimits = FinSettings.Limits(range: 0...100, step: 0.5)
    static let minHeightLimits = FinSettings.Limits(range: 0...1000, step: 1)
    static let bandWidthLimits = FinSettings.Limits(range: 0.1...20, step: 0.1)
    static let tiesLimits = 1...20
    static let contactLimits = FinSettings.Limits(range: 0.05...10, step: 0.05)
    static let middleLimits = FinSettings.Limits(range: 0.05...20, step: 0.05)
    /// The height range in percent of the height the two models share.
    static let percentLimits = FinSettings.Limits(range: 0...100, step: 5)

    /// The JSON the engine takes.
    var json: String {
        let enc = JSONEncoder()
        enc.outputFormatting = [.sortedKeys]
        return (try? enc.encode(self)).map { String(decoding: $0, as: UTF8.self) } ?? ""
    }

    /// The settings `text` (the engine's JSON) holds, or nil.
    static func decode(_ text: String) -> TieSettings? {
        try? JSONDecoder().decode(TieSettings.self, from: Data(text.utf8))
    }

    /// `settings` over printer `profile`'s defaults, checked by the engine: nil settings are the profile's defaults
    /// ("Reset to profile defaults" passes ""). Throws the engine's refusal, which names the key.
    static func validated(_ settings: TieSettings?, profile: String) throws -> TieSettings {
        let text = try plateTiesSettings(profileId: profile, settingsJson: settings?.json ?? "")
        guard let s = decode(text) else { throw FfiError.Engine(message: "plate ties: unreadable settings") }
        return s
    }
}

/// One plate's ties: on or off, its settings (nil: the printer profile's defaults) and the user's changes to the links:
/// pairs clicked off, and pairs added by clicking two models. Keys are the models' slot ids (`ModelSlot.id`).
struct PlateTieState: Equatable {
    var on = false
    var settings: TieSettings?
    var disabled: [TiePairKey] = []
    var added: [TiePairKey] = []

    var overrides: TieOverrides { TieOverrides(disabled: disabled, added: added) }

    /// The overrides for a plate of models `keys`: an added pair naming a model no longer on it is dropped (the engine
    /// refuses one); a disabled one stays, the engine ignores it.
    func overrides(on keys: Set<String>) -> TieOverrides {
        TieOverrides(disabled: disabled, added: added.filter { keys.contains($0.a) && keys.contains($0.b) })
    }
}

extension TiePairKey {
    /// The same two models, either order.
    func same(_ a: String, _ b: String) -> Bool { (self.a == a && self.b == b) || (self.a == b && self.b == a) }
}

/// A plate's ties as an export (or the view) asks for them: what the engine builds them from. Built off the main
/// thread, on the very models written, so the merged 3MF always matches (`export_plate_tied` checks it).
struct TieRequest: Sendable {
    var profile: String
    var settings: String
    var overrides: TieOverrides

    /// The plate measured, the pairs found and their ties built. Synchronous FFI: never on the main actor.
    nonisolated func plate(models: [PlateModel], keys: [String]) throws -> TiePlate {
        try TiePlate(models: zip(keys, models).map { TieModelInput(key: $0, model: $1) }, profileId: profile)
    }

    nonisolated func build(models: [PlateModel], keys: [String]) throws -> TieBuildResult {
        try plate(models: models, keys: keys).build(settingsJson: settings, overrides: overrides)
    }
}

/// What one detect-and-build of the plate being edited found (`PlateTiesModel.rebuild`).
struct TieOutcome: Sendable {
    /// The pairs tied, and every pair found (the ones clicked off too, drawn grey).
    var links: [TieLink]
    var candidates: [TieLink]
    var built: TieBuildResult
}

extension TieRequest {
    /// Measure, detect (with and without the pairs clicked off) and build: the view's ties.
    nonisolated func run(_ w: PlateBoard.PlateWrite) throws -> TieOutcome {
        let r = w.resolved()
        let plate = try plate(models: r.models, keys: r.keys)
        let links = try plate.detect(settingsJson: settings, overrides: overrides)
        let candidates = try plate.detect(settingsJson: settings, overrides: TieOverrides(disabled: [], added: overrides.added))
        let built = try plate.build(settingsJson: settings, overrides: overrides)
        return TieOutcome(links: links, candidates: candidates, built: built)
    }
}

/// The words of a tie note (`TieNoteInfo`), in the app's language; the engine's `text` is English for logs and the CLI.
enum TieNotes {
    static func text(_ n: TieNoteInfo, pair: String, model: String?) -> String {
        let tie = n.tie.map { String($0 + 1) } ?? "?"
        switch n.kind {
        case "face-fallback":
            return JS.t("{pair}: tie {tie} went on the face: the edge band had no room for it", ["pair": pair, "tie": tie])
        case "shallow-sink":
            return JS.t("{pair}: tie {tie} sinks half as deep: the section there is thin", ["pair": pair, "tie": tie])
        case "skipped":
            return JS.t("{pair}: tie {tie} left out: {why}", ["pair": pair, "tie": tie, "why": reason(n.reason, model: model)])
        case "no-room":
            return JS.t("{pair}: no room for ties above the bed", ["pair": pair])
        default:
            return "\(pair): \(n.text)"
        }
    }

    static func reason(_ code: String?, model: String?) -> String {
        let name = model ?? "?"
        switch code {
        case "no-overlap": return JS.t("the models don't face each other at that height")
        case "missed": return JS.t("it would miss {name}", ["name": name])
        case "too-long": return JS.t("the gap is too wide there")
        case "slope": return JS.t("{name} slopes too much there", ["name": name])
        case "edge": return JS.t("it would sit on an edge of {name}", ["name": name])
        case "blocked": return JS.t("another model or a support is in the way")
        default: return code ?? "?"
        }
    }
}
