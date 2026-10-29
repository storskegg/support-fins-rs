import XCTest

/// Help through the real menu bar: the system's "Support Fins Help" item is there and enabled (it is enabled only when the
/// Help Book is registered, `CFBundleHelpBookFolder`), followed by the app's own items. Choosing it opens the Help Viewer,
/// another app whose timing this test does not depend on.
final class HelpUITests: PartWindowUITestCase {
    func testTheHelpMenuHasTheHelpBookAndTheAppsItems() throws {
        try openFixturePart()
        let menu = menuBarItem("Help")
        menu.click()
        let help = menu.menuItems["Support Fins Help"]
        XCTAssertTrue(help.waitForExistence(timeout: 5), "the system's Help item")
        XCTAssertTrue(help.isEnabled, "the Help Book is registered")
        for title in ["Calibration Prints…", "Full Calibration Guide", "Support Fins on GitHub", "Support the project", "Report a problem"] {
            XCTAssertTrue(menu.menuItems[title].exists, title)
        }
        app.typeKey(.escape, modifierFlags: [])
    }
}
