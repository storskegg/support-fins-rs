import XCTest

/// The settings search (`web/ui/search.js`) through the real window: ⌥⌘F focuses the box, a query narrows the
/// inspector to the rows that match, "No settings match" says when none does, and clearing puts the panel back.
final class SearchUITests: PartWindowUITestCase {
    func testTypingNarrowsTheInspectorAndClearingBringsItBack() throws {
        try openFixturePart()
        showInspectorTab(.supports)
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

    /// A search shows the matches of every tab: "gap" finds Support gap (Supports) and the Bed pad that reveals Pad gap
    /// (More), each tab's after a button; the button shows them in their tab, with the box cleared.
    func testASearchReachesEveryTabAndShowsAMatchInItsTab() throws {
        try openFixturePart()
        XCTAssertTrue(element("stat-name").waitForExistence(timeout: 10), "the Part tab")

        menuBarItem("Edit").click()
        app.menuItems["Search Settings"].click()
        app.typeText("support gap")
        let gap = element("support-gap"), supports = element("search-tab-supports")
        waitFor("Support gap shows from the Supports tab", timeout: 5) { gap.exists && supports.exists }
        XCTAssertFalse(element("search-tab-more").exists, "nothing in More matches")

        app.typeKey("a", modifierFlags: .command)
        app.typeText("gap")
        let more = element("search-tab-more")
        waitFor("More's match shows too", timeout: 5) { gap.exists && more.exists }

        supports.click()
        waitFor("the Supports tab, the box cleared", timeout: 5) {
            gap.exists && self.element("fin-mode-auto").exists && !supports.exists && !more.exists
        }
        XCTAssertEqual(element("opts-search").value as? String ?? "", "", "the box is empty")
        XCTAssertTrue(gap.isHittable, "scrolled to Support gap")
    }
}
