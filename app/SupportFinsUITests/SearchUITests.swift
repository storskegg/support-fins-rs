import XCTest

/// The settings search (`web/ui/search.js`) through the real window: ⌥⌘F focuses the box, a query narrows the
/// inspector to the rows that match, "No settings match" says when none does, and clearing puts the panel back.
final class SearchUITests: PartWindowUITestCase {
    func testTypingNarrowsTheInspectorAndClearingBringsItBack() throws {
        try openFixturePart()
        let placement = element("fin-mode-auto"), grip = element("tine-density")
        XCTAssertTrue(placement.waitForExistence(timeout: 10) && grip.waitForExistence(timeout: 5), "the whole panel shows")

        menuBarItem("Edit").click()
        app.menuItems["Search Settings"].click()
        app.typeText("placement")
        waitFor("only Placement is left", timeout: 5) { placement.exists && !grip.exists }

        app.typeKey("a", modifierFlags: .command)
        app.typeText("zzzz")
        waitFor("nothing matches", timeout: 5) { self.element("opts-search-empty").exists && !placement.exists }

        app.typeKey(.escape, modifierFlags: [])      // clears the box
        waitFor("the panel is back", timeout: 5) { placement.exists && grip.exists && !self.element("opts-search-empty").exists }
    }
}
