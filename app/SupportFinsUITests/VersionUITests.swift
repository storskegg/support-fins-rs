import XCTest

/// The app's version and build number where a user looks for them (docs/AGENTS.md "Versioning and releases"): the last
/// line of the Help menu, and the standard About Support Fins panel ("Version 2026.44.0 (1234)": the bundle's
/// CFBundleShortVersionString and CFBundleVersion, which the Build number script sets).
final class VersionUITests: PartWindowUITestCase {
    /// "Version <semver> (<build>)", the build a positive number (the script ran; it is 0 without it)
    static var shown: NSPredicate {
        NSPredicate(format: "title MATCHES %@ OR value MATCHES %@ OR label MATCHES %@", pattern, pattern, pattern)
    }
    static let pattern = #"Version \d+\.\d+\.\d+ \([1-9]\d*\)"#

    func testTheHelpMenuEndsWithTheVersionAndBuild() throws {
        try openFixturePart()
        let menu = menuBarItem("Help")
        menu.click()
        let line = menu.menuItems.matching(Self.shown).firstMatch
        XCTAssertTrue(line.waitForExistence(timeout: 5), "Help's last line names the version and the build")
        app.typeKey(.escape, modifierFlags: [])
    }

    func testAboutSupportFinsShowsTheVersionAndBuild() throws {
        try openFixturePart()
        let menu = menuBarItem("Support Fins")
        menu.click()
        let about = menu.menuItems["About Support Fins"]
        XCTAssertTrue(about.waitForExistence(timeout: 5), "the app menu's About item")
        about.click()
        let text = app.staticTexts.matching(Self.shown).firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 5), "the About panel names the version and the build")
        app.typeKey("w", modifierFlags: .command)
    }
}
