import XCTest

/// The app in the web's other languages (`Resources/Localizable.xcstrings`): launched with `-AppleLanguages`, the
/// window shows the web's translations. The launch, the window and the helpers are `PartWindowUITestCase`'s.
final class LanguageUITests: PartWindowUITestCase {
    /// Relaunch the app in a language: set before `openFixturePart()` launches it.
    ///
    /// `-AppleTextDirection YES` lets the language set the layout direction, as on a Mac whose language is Arabic.
    /// It does not force right to left (English and German stay left to right with it); without it a Mac with
    /// `AppleTextDirection = 0` in its global preferences (`/Library/Preferences/.GlobalPreferences`, as on the
    /// test Mac) keeps every app left to right whatever its language, and the Arabic window was not mirrored.
    private func launch(in language: String, locale: String) throws {
        app.launchArguments += ["-AppleLanguages", "(\(language))", "-AppleLocale", locale, "-AppleTextDirection", "YES"]
        try openFixturePart()
    }

    func testGermanShowsTheWebsTranslations() throws {
        try launch(in: "de", locale: "de_DE")
        // the toolbar's Add fins and the Scale badge (`fins-toggle`, `toolbar-scale-badge`)
        XCTAssertTrue(element("fins-toggle").waitForExistence(timeout: 10))
        XCTAssertEqual(element("fins-toggle").label, "Finnen hinzufügen")
        XCTAssertEqual(element("toolbar-scale-badge").label, "Skalieren")
        XCTAssertEqual(element("toolbar-rotate").label, "Drehen")
        // left to right: the build volume, the toolbar's first item, is left of Add fins
        XCTAssertLessThan(element("volume").frame.midX, element("fins-toggle").frame.midX)
    }

    func testArabicShowsTheWebsTranslationsAndMirrorsTheLayout() throws {
        try launch(in: "ar", locale: "ar_SA")
        XCTAssertTrue(element("fins-toggle").waitForExistence(timeout: 10))
        XCTAssertEqual(element("fins-toggle").label, "إضافة الزعانف")
        XCTAssertEqual(element("toolbar-scale-badge").label, "تحجيم")
        XCTAssertEqual(element("toolbar-rotate").label, "تدوير")
        // right to left: the build volume, the toolbar's first item, sits to the right of Add fins (it is to its left in
        // German); SwiftUI mirrors the window itself, the app forces no direction. The 3D view and the view cube stay
        // left to right (`ContentView`, `ViewCubeView`)
        XCTAssertGreaterThan(element("volume").frame.midX, element("fins-toggle").frame.midX)
        // the inspector moves to the window's left: its left edge is left of the viewport's centre
        XCTAssertLessThan(element("stat-name").frame.minX, element("viewport").frame.midX)
    }
}
