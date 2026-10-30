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

    /// A plate keeps its model's cut while another plate is edited (`sectionState` / `restoreSection`,
    /// `web/ui/models.js:68,104`): on with the first plate's model, off on the second plate's, on again back on the
    /// first. The badge says whether the cut is on (selected).
    func testAPlateKeepsItsCutWhileAnotherIsEdited() throws {
        let first = try writeSTL("first.stl", slab: 30)
        let second = try writeSTL("second.stl", slab: 20)
        try openPart(first)
        let name = element("stat-name")
        XCTAssertTrue(name.waitForExistence(timeout: 20), "the File row")
        waitFor("the File row names the first model", timeout: 20) { self.text(name).hasPrefix("first") }

        // the cut on, its popover folded
        let badge = element("toolbar-section")
        XCTAssertTrue(badge.waitForExistence(timeout: 10), "the Section badge")
        badge.click()
        XCTAssertTrue(popoverElement("section-flip").waitForExistence(timeout: 5), "the popover opened with the cut on")
        popoverElement("section-flip").click()
        app.typeKey(.escape, modifierFlags: [])
        waitFor("the cut is on, its popover folded", timeout: 5) { badge.isSelected && !self.popoverElement("section-flip").exists }

        // a second plate with a model of its own: no cut there
        let tab1 = element("plate-tab-1"), tab2 = element("plate-tab-2")
        element("plate-add").click()
        XCTAssertTrue(tab2.waitForExistence(timeout: 5), "the second plate's tab")
        let importButton = element("empty-import")
        XCTAssertTrue(importButton.waitForExistence(timeout: 5), "an empty plate offers Import")
        importButton.click()
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel for the new plate")
        chooseInPanel(second.path)
        waitFor("the second plate's model, without a cut", timeout: 20) {
            self.text(name).hasPrefix("second") && badge.exists && !badge.isSelected
        }

        // back to the first plate: its cut is back
        tab1.click()
        waitFor("the first plate's model, with its cut", timeout: 10) { self.text(name).hasPrefix("first") && badge.isSelected }
        tab2.click()
        waitFor("the second plate's model, still without one", timeout: 10) { self.text(name).hasPrefix("second") && !badge.isSelected }
        tab1.click()
        waitFor("the first plate's cut again", timeout: 10) { self.text(name).hasPrefix("first") && badge.isSelected }
    }
}
