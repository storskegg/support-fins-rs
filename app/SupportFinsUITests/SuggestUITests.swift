import XCTest

/// Suggest orientation through the real window (`web/ui/suggest.js`): the toolbar button opens the popover, the
/// button ranks on click (never on open) and reads `Ranking…`, with a percentage once the engine reports, while it
/// works; then the rows of the ranking are listed.
final class SuggestUITests: PartWindowUITestCase {
    func testTheButtonRanksAndTheRowsAppear() throws {
        try openFixturePart()

        let slot = element("toolbar-suggest")
        XCTAssertTrue(slot.waitForExistence(timeout: 10))
        slot.click()
        let button = popoverElement("suggest-orient")
        XCTAssertTrue(button.waitForExistence(timeout: 5), "the popover opened")
        XCTAssertEqual(button.label, "Suggest orientation")
        XCTAssertFalse(popoverElement("suggest-row").exists, "opening the popover ranks nothing")

        button.click()
        // the label is `Ranking…` (then `Ranking… 40%`) only while it runs: a small part may be done before it is read
        waitFor("the ranking is listed", timeout: 60) { self.popoverElement("suggest-row").exists }
        XCTAssertEqual(button.label, "Suggest orientation", "the button is back once the ranking is in")
        XCTAssertTrue(button.isEnabled)
        XCTAssertTrue(popoverElement("suggest-note").exists)
    }
}
