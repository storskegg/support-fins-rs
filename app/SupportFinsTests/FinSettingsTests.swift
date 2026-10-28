import Foundation
import Testing
@testable import Support_Fins

/// `FinSettings.optionsJSON()` against the web: `parity/corpus/settings/optionsJSON.jsonl`, recorded by
/// `parity/oracle/settings.js` from the web's `finOpts` / `swayOpts` and the engine entry's `optionsFromDialog`.
@Suite("FinSettings")
struct FinSettingsTests {
    private struct Case {
        let id: String
        let form: Data
        let dialog: [String: Any]
    }

    private static func corpus() throws -> [Case] {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let text = try String(contentsOf: root.appending(path: "parity/corpus/settings/optionsJSON.jsonl"), encoding: .utf8)
        return try text.split(separator: "\n").map { line in
            let obj = try #require(try JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any])
            let args = try #require(obj["args"] as? [Any])
            let out = try #require(obj["out"] as? [String: Any])
            return Case(id: try #require(obj["id"] as? String),
                        form: try JSONSerialization.data(withJSONObject: args[0]),
                        dialog: try #require(out["dialog"] as? [String: Any]))
        }
    }

    /// JSON values of the same kind and value (a bool is not the number 1).
    private static func same(_ a: Any, _ b: Any) -> Bool {
        if let x = a as? NSNumber, let y = b as? NSNumber {
            let xb = CFGetTypeID(x) == CFBooleanGetTypeID(), yb = CFGetTypeID(y) == CFBooleanGetTypeID()
            return xb == yb && x.doubleValue == y.doubleValue
        }
        if let x = a as? String, let y = b as? String { return x == y }
        return false
    }

    private func dictionary(_ json: String) throws -> [String: Any] {
        try #require(try JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any])
    }

    @Test func optionsJSONMatchesTheWebForEveryRecordedCase() throws {
        let cases = try Self.corpus()
        #expect(cases.count >= 30)
        for c in cases {
            let settings = try JSONDecoder().decode(FinSettings.self, from: c.form)
            let got = try dictionary(settings.optionsJSON())
            #expect(Set(got.keys) == Set(c.dialog.keys), "\(c.id): keys")
            for (key, want) in c.dialog {
                let have = try #require(got[key], "\(c.id): \(key) missing")
                #expect(Self.same(have, want), "\(c.id): \(key) = \(have), the web sends \(want)")
            }
        }
    }

    @Test func theDefaultsAreTheWebDefaults() throws {
        let d = FinSettings()
        #expect(d.threshold == 45 && d.nozzle == 0.4 && d.layerHeight == 0.2 && d.gap == 0.2)
        #expect(d.mode == .auto && d.tines && d.tineDensity == 0 && d.tineDirection == .horizontal && d.pinDepth == 0.1)
        #expect(d.cutout == .none && !d.roundedFeet && d.bedPad == .auto && d.wallLines == 2)
        #expect(d.padThickness == 0.2 && d.padGap == 0.12 && d.padGrip == 0 && d.padSpread == 4)
        #expect(d.tallRibs && d.tallRibsHeight == 10 && !d.sway && d.swayFrom == 0 && d.swaySpacing == 6)
        #expect(d.swayDepth == 15 && !d.fullBraceDepth && d.swayCrossHeight == 50)
        #expect(d.coverage == 50 && !d.plateOnly && !d.separateWalls && d.longWalls && !d.curvedWalls)
        #expect(d.highlightSmall && !d.showIslands && d.interfaceMaterial == .off)
        #expect(d.notSentToEngine.isEmpty)
        let json = try dictionary(d.optionsJSON())
        #expect(json.count == 16)
        #expect(Self.same(json["coverage"]!, 0.5) && Self.same(json["sway.on"]!, false) && Self.same(json["mode"]!, "auto"))
    }

    @Test func aMissingKeyTakesTheDefaultAndTheEncodingIsComplete() throws {
        let partial = try JSONDecoder().decode(FinSettings.self, from: Data(#"{"nozzle":0.6,"mode":"full"}"#.utf8))
        var want = FinSettings()
        want.nozzle = 0.6
        want.mode = .full
        #expect(partial == want)
        let round = try JSONDecoder().decode(FinSettings.self, from: JSONEncoder().encode(want))
        #expect(round == want)
        #expect(try dictionary(String(decoding: JSONEncoder().encode(want), as: UTF8.self)).count == 33)
        #expect(throws: DecodingError.self) { try JSONDecoder().decode(FinSettings.self, from: Data(#"{"mode":"sideways"}"#.utf8)) }
    }

    @Test func settingsTheEngineEntryCannotTakeAreListedNotSilentlyDropped() {
        var s = FinSettings()
        s.roundedFeet = true
        s.wallLines = 4
        s.tallRibs = false
        s.curvedWalls = true
        s.bedPad = .custom
        s.padGap = 0
        s.interfaceMaterial = .flat
        #expect(s.notSentToEngine == ["roundedFeet", "bedPad=custom", "padGap", "wallLines", "tallRibs", "curvedWalls", "interfaceMaterial"])
        var t = FinSettings()
        t.highlightSmall = false // display only: nothing to send
        #expect(t.notSentToEngine.isEmpty)
    }

    @Test func drawSendsNoModeAndNoSwayBraces() throws {
        var s = FinSettings()
        s.mode = .draw
        s.sway = true
        let json = try dictionary(s.optionsJSON())
        #expect(json["mode"] == nil && json["sway.gripFrom"] == nil)
        #expect(Self.same(json["sway.on"]!, false))
    }
}
