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
        // On any failure below, the accessibility tree of the whole app is attached, so a failed run says what the
        // popover held (rows missing, rows folded into another element, or the ranking's error text).
        func attachTree(_ why: String) {
            let tree = XCTAttachment(string: "\(why)\n\n\(self.app.debugDescription)")
            tree.name = "accessibility tree: \(why)"
            tree.lifetime = .keepAlways
            self.add(tree)
        }
        // First the ranking itself: the label is `Ranking…` (then `Ranking… 40%`) only while it runs, and the fixture
        // is tiny, so it may be over before the first read.
        let rows = popoverElement("suggest-row")
        let note = popoverElement("suggest-note")
        let done = waitUntil(timeout: 60) { rows.exists || note.exists }
        if !done || !rows.exists {
            attachTree("no rows; popover button '\(button.label)', note '\(note.exists ? note.label : "(none)")', popover exists \(app.popovers.count)")
        }
        XCTAssertTrue(done, "the ranking produced neither rows nor a note within 60 s")
        XCTAssertTrue(rows.exists, "the ranking is listed (note: \(note.exists ? note.label : "none"))")
        XCTAssertEqual(button.label, "Suggest orientation", "the button is back once the ranking is in")
        XCTAssertTrue(button.isEnabled)
        XCTAssertTrue(note.exists)
    }
}
