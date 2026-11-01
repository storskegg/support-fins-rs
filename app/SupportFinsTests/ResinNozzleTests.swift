import Foundation
import SupportFinsCore
import Testing

@testable import Support_Fins

/// A resin printer has no nozzle: its Nozzle row is hidden (#255), and the build takes the 0.4 mm lines whatever nozzle
/// was saved for an FDM printer. The saved nozzle stays, so an FDM printer picked again builds with it.
@MainActor
@Suite("Resin printers build with a 0.4 mm nozzle")
struct ResinNozzleTests {
    static func profile(_ kind: String) -> PrinterProfileInfo {
        PrinterProfileInfo(id: kind, name: kind, kind: kind, isDefault: kind == "resin", buildVolume: [200, 200, 200],
                           pixelSize: kind == "resin" ? [0.014, 0.019] : [], nozzle: kind == "fdm" ? 0.4 : nil,
                           layerHeightMin: 0.01, layerHeightMax: 0.3, layerHeightDefault: 0.05, minTieThickness: 0.3,
                           plateTiesJson: "{}", sourceJson: "{}")
    }

    /// The `nozzle` of an options or tunables JSON.
    static func nozzle(_ json: String) -> Double? {
        (try? JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any])?["nozzle"] as? Double
    }

    static func saved08() -> FinSettings {
        var s = FinSettings()
        s.setNozzle(0.8)
        return s
    }

    @Test func aResinPrinterBuildsWith04AndAnFDMPrinterWithTheSavedNozzle() {
        let s = Self.saved08()
        let resin = s.forBuild(on: Self.profile("resin"))
        #expect(Self.nozzle(resin.optionsJSON()) == 0.4)
        #expect(Self.nozzle(resin.tunablesJSON()) == 0.4, "the tunables carry the nozzle too (Draw's walls, Suggest)")
        #expect(Self.nozzle(s.forBuild(on: Self.profile("fdm")).optionsJSON()) == 0.8)
        #expect(Self.nozzle(s.forBuild(on: nil).optionsJSON()) == 0.8, "no profile: the settings as they are")
        #expect(s.nozzle == 0.8, "the saved nozzle is untouched")
        // only the nozzle changes
        var expected = s
        expected.nozzle = 0.4
        #expect(resin == expected)
    }

    @Test func switchingPrintersRestoresTheSavedNozzle() throws {
        let suite = "resin-nozzle-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = PrinterProfileStore(defaults: defaults, profiles: [Self.profile("resin"), Self.profile("fdm")])
        let s = Self.saved08()
        #expect(Self.nozzle(s.forBuild(store).optionsJSON()) == 0.4, "the default resin printer")
        store.choose("fdm")
        #expect(Self.nozzle(s.forBuild(store).optionsJSON()) == 0.8, "an FDM printer: the saved nozzle")
        store.choose("resin")
        #expect(Self.nozzle(s.forBuild(store).optionsJSON()) == 0.4)
    }

    /// The window's build asks the engine for 0.4 under the default printer (the Saturn 4 Ultra 16K, resin) with 0.8
    /// saved; the session keeps 0.8 (it is what Settings and a session file hold).
    @Test func theWindowsBuildRequestTakes04UnderTheDefaultResinPrinter() async throws {
        try #require(PrinterProfileStore.shared.current?.kind == "resin", "the default profile is a resin printer's")
        let model = ViewerModel()
        let stl = URL(fileURLWithPath: try TestParts.write(TestParts.box(-5, 5, -5, 5, 0, 10), name: "resin-nozzle-\(UUID().uuidString).stl"))
        defer { Scratch.remove(stl) }
        await model.load(url: stl)
        let wiring = try #require(model.wiring)
        model.session.settings.setNozzle(0.8)
        #expect(Self.nozzle(wiring.key.options) == 0.4)
        #expect(Self.nozzle(wiring.key.tunables) == 0.4)
        #expect(model.session.settings.nozzle == 0.8)
    }
}
