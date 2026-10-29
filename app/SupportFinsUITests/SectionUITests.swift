import XCTest

/// Section view through the real window (`web/ui/section.js`): the toolbar's Section badge turns the cut on and
/// opens its popover (Ghost / Hide, Flip, Re-aim), Esc folds the popover and leaves the cut on, the lit badge brings
/// it back, and clicking the badge with its popover open turns the cut off. The popover's controls are looked up
/// with `popoverElement` (a popover is a window of its own); the rest is `PartWindowUITestCase`'s.
final class SectionUITests: PartWindowUITestCase {
    func testSectionBadgeOpensThePopoverAndTurnsOffFromIt() throws {
        try openFixturePart()
        let badge = element("toolbar-section")
        XCTAssertTrue(badge.waitForExistence(timeout: 10), "the Section badge")
        badge.click()

        let ghost = popoverElement("section-ghost"), hide = popoverElement("section-hide")
        let flip = popoverElement("section-flip"), reaim = popoverElement("section-reset")
        XCTAssertTrue(ghost.waitForExistence(timeout: 5), "the popover opened with the cut on")
        XCTAssertTrue(hide.exists && flip.exists && reaim.exists)
        hide.click()
        flip.click()
        reaim.click()
        XCTAssertTrue(hide.exists, "still open")

        // Esc folds the popover whichever window has the keys (the toolbar watches the app's key events)
        app.typeKey(.escape, modifierFlags: [])
        waitFor("Esc folds the popover", timeout: 5) { !ghost.exists && !flip.exists }
        badge.click()                                   // the lit badge brings its settings back
        XCTAssertTrue(ghost.waitForExistence(timeout: 5), "the popover is back")
        badge.click()                                   // with the popover open: off
        waitFor("the cut is off", timeout: 5) { !ghost.exists }
    }
}
