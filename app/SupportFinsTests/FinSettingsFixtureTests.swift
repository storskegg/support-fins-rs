import Foundation
import Testing
@testable import Support_Fins

/// The texts the app sends the engine, as checked-in files (`parity/fixtures/app-settings/*.json`) that the Rust
/// side reads too: `crates/sf-ffi/tests/app_settings.rs` runs every file through every JSON-taking FFI entry.
/// This test is the other half: `FinSettings` must still produce each file's texts byte for byte, so a key added,
/// renamed or reformatted on this side fails here until the fixture (and so the Rust test's input) is regenerated.
///
/// To regenerate after a deliberate change: a failing run writes what `FinSettings` produces for each variant to
/// `<temp>/app-settings/` and names the folder in the failure; copy the files over the fixtures and review the diff.
/// (The test host is sandboxed, so it cannot write into the repository itself.)
@Suite("FinSettings fixtures")
struct FinSettingsFixtureTests {
    /// The variants: the defaults and the settings that change the request's shape. 4 wall lines, not 3: the
    /// Wall lines menu offers 2, 4, 6 and 8 and the engine's FFI refuses anything else.
    static let variants: [(name: String, settings: FinSettings)] = {
        var draw = FinSettings(); draw.mode = .draw
        var sway = FinSettings()
        sway.sway = true; sway.swayFrom = 10; sway.swaySpacing = 8; sway.swayDepth = 20
        sway.fullBraceDepth = true; sway.swayCrossHeight = 60
        var pad = FinSettings()
        pad.bedPad = .custom; pad.padThickness = 0.4; pad.padGap = 0.2; pad.padGrip = 0.1; pad.padSpread = 6
        var nozzle = FinSettings(); nozzle.nozzle = 0.8; nozzle.wallLines = 4
        return [("default", FinSettings()), ("draw", draw), ("sway", sway), ("custom-pad", pad),
                ("nozzle-0.8-wall-lines-4", nozzle)]
    }()

    private static var directory: URL {
        URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appending(path: "parity/fixtures/app-settings")
    }

    /// The fixture file's text for `settings`: sorted keys, so the file is stable.
    private static func fixture(_ s: FinSettings) throws -> String {
        let enc = JSONEncoder()
        enc.outputFormatting = [.sortedKeys]
        let obj: [String: Any] = [
            "settings": try JSONSerialization.jsonObject(with: enc.encode(s)),
            "optionsJSON": s.optionsJSON(),
            "tunablesJSON": s.tunablesJSON(),
            "drawSettingsJSON": s.drawSettingsJSON(),
            "sessionJSON": SessionFile.json(settings: s, finsOn: true, walls: [], removed: []),
        ]
        let data = try JSONSerialization.data(withJSONObject: obj, options: [.sortedKeys, .prettyPrinted, .withoutEscapingSlashes])
        return String(decoding: data, as: UTF8.self) + "\n"
    }

    @Test func settingsStillProduceTheCheckedInFixtures() throws {
        let out = FileManager.default.temporaryDirectory.appending(path: "app-settings")
        try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
        for (name, settings) in Self.variants {
            let want = try Self.fixture(settings)
            try want.write(to: out.appending(path: "\(name).json"), atomically: true, encoding: .utf8)
            let have = try String(contentsOf: Self.directory.appending(path: "\(name).json"), encoding: .utf8)
            #expect(have == want, "\(name): FinSettings no longer produces parity/fixtures/app-settings/\(name).json; the new text is in \(out.path()), copy it over and review the diff")
        }
    }

    @Test func everyFixtureFileHasAVariant() throws {
        let files = try FileManager.default.contentsOfDirectory(atPath: Self.directory.path()).filter { $0.hasSuffix(".json") }
        #expect(Set(files) == Set(Self.variants.map { "\($0.name).json" }))
    }

    @Test func theDefaultFixtureIsTheDefaultSettings() throws {
        let text = try String(contentsOf: Self.directory.appending(path: "default.json"), encoding: .utf8)
        let obj = try #require(try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        #expect(obj["optionsJSON"] as? String == FinSettings().optionsJSON())
        #expect(obj["tunablesJSON"] as? String == FinSettings().tunablesJSON())
    }
}
