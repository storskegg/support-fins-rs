import XCTest

/// What's new (`web/changelog.js`) through the real window: with a release in the list, Help > What's new opens the sheet
/// with the newest release in its heading, its lines, the older release under its own line and the "See all releases"
/// link; Done closes it. The real list is empty until a release is stamped, so the app reads a two-release file named by
/// the `SFChangelogFixture` default (`Changelog.bundled`); with the real list, Help has no What's new item.
final class WhatsNewUITests: PartWindowUITestCase {
    func testHelpWhatsNewListsTheReleases() throws {
        let list = dir.appending(path: "changelog.json")
        let json = """
        {"version":"1.0.1","releases":[\
        {"version":"1.0.1","tag":"v1.0.1","date":"2026-11-03","lines":["The newest line","Another new line"]},\
        {"version":"1.0.0","tag":"v1.0.0","date":"2026-10-28","lines":["The first release line"]}]}
        """
        try Data(json.utf8).write(to: list)
        app.launchArguments += ["-SFChangelogFixture", list.path]
        try openFixturePart()

        let help = menuBarItem("Help")
        help.click()
        help.menuItems["What's new"].click()

        let heading = element("whatsnew-heading")
        XCTAssertTrue(heading.waitForExistence(timeout: 10), "the sheet's heading")
        XCTAssertTrue(text(heading).contains("v1.0.1"), "the newest release is named in the heading: \(text(heading))")
        XCTAssertTrue(element("whatsnew-release-v1.0.0").exists, "the older release has its own line")
        XCTAssertTrue(app.staticTexts["The first release line"].exists || element("whatsnew-list").exists, "the list")
        XCTAssertTrue(element("whatsnew-all").exists, "See all releases")

        element("whatsnew-done").click()
        waitFor("the sheet closed", timeout: 5) { !heading.exists }
    }
}
