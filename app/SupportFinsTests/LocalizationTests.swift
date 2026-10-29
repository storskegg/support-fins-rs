import Foundation
import Testing
@testable import Support_Fins
import Viewer

/// The app's strings in the six languages (`Resources/Localizable.xcstrings`, generated from the web's tables by
/// `tools/i18n`): the web's own translations come out of `JS.t` / `JS.tn`, plurals follow each language's rules, and a
/// text that never shows its count still varies by it. `tools/i18n/parity.ts` checks the catalog against the web.
@Suite("Localization")
struct LocalizationTests {
    private func lang(_ code: String) throws -> L10n.Language { try #require(L10n.Language(code), "the app has no \(code) strings") }

    @Test("the app carries the web's six languages")
    func sixLanguages() throws {
        for code in ["en", "de", "zh-Hans", "pt", "ar", "nl"] { _ = try lang(code) }
    }

    @Test("plain strings are the web's translations")
    func plain() throws {
        #expect(JS.t("Draw", in: try lang("de")) == "Zeichnen")
        #expect(JS.t("Add fins", in: try lang("de")) == "Finnen hinzufügen")
        #expect(JS.t("Add fins", in: try lang("ar")) == "إضافة الزعانف")
        #expect(JS.t("Add fins", in: try lang("zh-Hans")) == "添加鳍片")
        #expect(JS.t("Add fins", in: try lang("pt")) == "Adicionar aletas")
        #expect(JS.t("Add fins", in: try lang("nl")) == "Vinnen toevoegen")
        #expect(JS.t("Add fins", in: try lang("en")) == "Add fins")
        // no entry for the language: English, as on the web
        #expect(JS.t("a string nobody translated", in: try lang("de")) == "a string nobody translated")
    }

    @Test("placeholders are filled in, and a translation may reorder them")
    func placeholders() throws {
        #expect(JS.t("Plate {n}", ["n": "3"], in: try lang("de")) == "Platte 3")
        #expect(JS.t("Plate {n}", ["n": "3"], in: try lang("en")) == "Plate 3")
        let amber = JS.t("Amber: overhangs under {area} mm², too small for a fin", ["area": "12"], in: try lang("de"))
        #expect(amber.contains("12") && !amber.contains("{"))
        // a string with two arguments
        let two = JS.t("merged {n} of {total} objects into one part", ["n": "2", "total": "5"], in: try lang("de"))
        #expect(two.contains("2") && two.contains("5") && !two.contains("{"))
        // a parameter the caller left out stays as the web leaves it
        #expect(JS.t("Plate {n}", [:], in: try lang("en")) == "Plate {n}")
    }

    @Test("a percent sign survives")
    func percent() throws {
        let tip = Tip.swayDepth
        #expect(tip.contains("15%") && !tip.contains("%%"))
        let de = JS.t("How far the brace reaches out at the bed, as a share of its height. Deeper = stiffer, more plastic. Default 15%.", in: try lang("de"))
        #expect(de.hasSuffix("Standard 15 %.") && !de.contains("%%"))
        let pt = JS.t("How far the brace reaches out at the bed, as a share of its height. Deeper = stiffer, more plastic. Default 15%.", in: try lang("pt"))
        #expect(pt.hasSuffix("Padrão 15%.") && !pt.contains("%%"))
        let withArg = JS.t("Lots of small or tucked-in overhang here ({pct} % bare). Try Full coverage; for figures and minis, slicer tree supports often do better.", ["pct": "60"], in: try lang("de"))
        #expect(withArg.contains("60") && !withArg.contains("%%"))
    }

    @Test("plurals follow the language's rules")
    func plurals() throws {
        let de = try lang("de"), en = try lang("en"), ar = try lang("ar"), zh = try lang("zh-Hans")
        func fins(_ n: Int, _ l: L10n.Language) -> String { JS.tn("{n} fin", "{n} fins", n, in: l) }
        #expect(fins(1, en) == "1 fin")
        #expect(fins(0, en) == "0 fins")
        #expect(fins(2, en) == "2 fins")
        #expect(fins(1, de) == "1 Finne")
        #expect(fins(5, de) == "5 Finnen")
        // Arabic has six forms: the web's object has `one`, `two` and `other` for this string
        #expect(fins(1, ar) != fins(2, ar))
        #expect(fins(2, ar) != fins(11, ar))
        #expect(fins(7, zh).replacingOccurrences(of: "7", with: "1") == fins(1, zh))
    }

    @Test("a plural with other arguments")
    func pluralWithArguments() throws {
        let de = try lang("de")
        let one = JS.tn("{n} drawn wall couldn’t attach here{reason}", "{n} drawn walls couldn’t attach here{reason}", 1, ["reason": " (X)"], in: de)
        let many = JS.tn("{n} drawn wall couldn’t attach here{reason}", "{n} drawn walls couldn’t attach here{reason}", 3, ["reason": " (X)"], in: de)
        #expect(one.contains("1") && one.contains("(X)") && !one.contains("{"))
        #expect(many.contains("3") && many.contains("(X)") && one != many)
    }

    @Test("a text that never shows its count still varies by it")
    func hiddenCount() throws {
        let de = try lang("de")
        let one = JS.tn("none needed: small hole left for bridging", "none needed: small holes left for bridging", 1, in: de)
        let two = JS.tn("none needed: small hole left for bridging", "none needed: small holes left for bridging", 2, in: de)
        #expect(one == "keine nötig: kleines Loch wird überbrückt")
        #expect(two == "keine nötig: kleine Löcher werden überbrückt")
        #expect(!one.contains(L10n.hiddenMark))
        let en = try lang("en")
        #expect(JS.tn("none needed: small hole left for bridging", "none needed: small holes left for bridging", 1, in: en) == "none needed: small hole left for bridging")
        #expect(JS.tn("none needed: small hole left for bridging", "none needed: small holes left for bridging", 3, in: en) == "none needed: small holes left for bridging")
    }

    @Test("native-only strings come with their own translations")
    func nativeOnly() throws {
        #expect(JS.t("Undo", in: try lang("de")) == "Rückgängig")
        #expect(JS.t("Rotate 90° About {axis}", ["axis": "X"], in: try lang("de")) == "90° um X drehen")
        #expect(JS.t("Lay a Face Flat", in: try lang("de")) == JS.t("Lay a face flat", in: try lang("de")))
    }

    @Test("the tips are the web's, translated")
    func tips() throws {
        #expect(JS.t(Tip.finMode, in: try lang("de")) != Tip.finMode)
    }
}

@Suite("Localization: the view cube")
struct ViewCubeLocalizationTests {
    @Test("the cube's face names come from the catalog (`ui/viewcube.js:59`)")
    func faces() throws {
        for code in ["de", "ar"] {
            let l = try #require(L10n.Language(code))
            for face in ViewCube.Face.allCases {
                let shown = face.localizedLabel(bundle: l.bundle, locale: l.locale)
                #expect(shown == JS.t(face.label, in: l), "\(code) \(face.label)")
            }
        }
        let de = try #require(L10n.Language("de"))
        #expect(ViewCube.Face.front.localizedLabel(bundle: de.bundle, locale: de.locale) == "Vorne")
        let ar = try #require(L10n.Language("ar"))
        #expect(ViewCube.Face.front.localizedLabel(bundle: ar.bundle, locale: ar.locale) != "Front")
        #expect(ViewCube.Face.front.localizedLabel(bundle: try #require(L10n.Language("en")).bundle) == "Front")
    }
}
