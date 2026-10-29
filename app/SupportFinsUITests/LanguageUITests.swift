import XCTest

/// The app in the web's other languages (`Resources/Localizable.xcstrings`): launched with `-AppleLanguages`, the
/// window shows the web's translations. The launch, the window and the helpers are `PartWindowUITestCase`'s.
final class LanguageUITests: PartWindowUITestCase {
    /// Relaunch the app in a language: set before `openFixturePart()` launches it.
    private func launch(in language: String, locale: String) throws {
        app.launchArguments += ["-AppleLanguages", "(\(language))", "-AppleLocale", locale]
        try openFixturePart()
    }

    func testGermanShowsTheWebsTranslations() throws {
        try launch(in: "de", locale: "de_DE")
        // the toolbar's Add fins and the Scale badge (`fins-toggle`, `toolbar-scale-badge`)
        XCTAssertTrue(element("fins-toggle").waitForExistence(timeout: 10))
        XCTAssertEqual(element("fins-toggle").label, "Finnen hinzufügen")
        XCTAssertEqual(element("toolbar-scale-badge").label, "Skalieren")
        XCTAssertEqual(element("toolbar-rotate").label, "Drehen")
    }

    func testArabicShowsTheWebsTranslationsAndMirrorsTheLayout() throws {
        try launch(in: "ar", locale: "ar_SA")
        XCTAssertTrue(element("fins-toggle").waitForExistence(timeout: 10))
        XCTAssertEqual(element("fins-toggle").label, "إضافة الزعانف")
        XCTAssertEqual(element("toolbar-scale-badge").label, "تحجيم")
        XCTAssertEqual(element("toolbar-rotate").label, "تدوير")
        // right to left: the build volume, the toolbar's first item, sits to the right of Add fins (it is to its left in
        // English); SwiftUI mirrors the window itself, the app forces no direction
        XCTAssertGreaterThan(element("volume").frame.midX, element("fins-toggle").frame.midX)
    }
}
