import XCTest

/// Section view through the real window (`web/ui/section.js`): the toolbar's Section badge turns the cut on and
/// opens its popover (Ghost / Hide, Flip, Re-aim), Hide is remembered as the selected choice, Esc folds the popover
/// and leaves the cut on, and clicking the badge with its popover open turns the cut off.
final class SectionUITests: PartWindowUITestCase {
    func testSectionBadgeOpensThePopoverAndTurnsOffFromIt() throws {
        try openFixturePart()
        let badge = element("toolbar-section")
        XCTAssertTrue(badge.waitForExistence(timeout: 10), "the Section badge")
        badge.click()

        let ghost = element("section-ghost"), hide = element("section-hide")
        XCTAssertTrue(ghost.waitForExistence(timeout: 5), "the popover opened with the cut on")
        XCTAssertTrue(hide.exists && element("section-flip").exists && element("section-reset").exists)
        hide.click()
        element("section-flip").click()
        element("section-reset").click()
        XCTAssertTrue(hide.exists, "still open")

        // Esc folds the popover whichever window has the keys (the toolbar watches the app's key events)
        app.typeKey(.escape, modifierFlags: [])
        let flip = element("section-flip")
        waitFor("Esc folds the popover", timeout: 5) { !ghost.exists && !flip.exists }
        badge.click()                                   // the lit badge brings its settings back
        XCTAssertTrue(ghost.waitForExistence(timeout: 5))
        badge.click()                                   // with the popover open: off
        waitFor("the cut is off", timeout: 5) { !ghost.exists }
    }
}
