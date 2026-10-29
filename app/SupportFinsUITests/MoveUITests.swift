import XCTest

/// Move through the real window: open a part, then turn the tool on and off with the toolbar's Move badge, the M
/// key, Esc and Model > Move. The badge says whether the tool is on by being selected. A drag across the plate needs
/// window coordinates, so it is left to the headless `MoveModelTests`. The launch, the window, `element` and
/// `menuBarItem` are `PartWindowUITestCase`'s; the menu bar is reached only through `app.menuBars`.
final class MoveUITests: PartWindowUITestCase {
    func testMoveTurnsOnAndOffFromTheBadgeTheKeyEscAndTheMenu() throws {
        try openFixturePart()

        let badge = element("toolbar-move")
        XCTAssertTrue(badge.waitForExistence(timeout: 10), "the Move badge")
        XCTAssertEqual(state(badge), "off")

        badge.click()
        waitFor("the badge turns Move on", timeout: 5) { self.state(badge) == "on" }
        badge.click()
        waitFor("the badge turns it off", timeout: 5) { self.state(badge) == "off" }

        // M toggles it, Esc leaves it
        app.typeKey("m", modifierFlags: [])
        waitFor("M turns it on", timeout: 5) { self.state(badge) == "on" }
        app.typeKey(.escape, modifierFlags: [])
        waitFor("Esc turns it off", timeout: 5) { self.state(badge) == "off" }

        // Model > Move
        menuBarItem("Model").click()
        app.menuBars.menuItems["Move"].click()
        waitFor("the menu turns it on", timeout: 5) { self.state(badge) == "on" }
        menuBarItem("Model").click()
        app.menuBars.menuItems["Move"].click()
        waitFor("the menu turns it off", timeout: 5) { self.state(badge) == "off" }
    }

    /// "on" while the badge is selected (its tool is on), else "off".
    private func state(_ e: XCUIElement) -> String { e.isSelected ? "on" : "off" }
}
