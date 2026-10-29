import XCTest

/// Scale through the real window (`web/ui/scale.js`): the Scale badge opens the popover, a typed percent plus
/// Return reloads the part and shows the percent on the badge, Reset (enabled only while scaled) brings it back.
/// The launch, the window and the helpers are `PartWindowUITestCase`'s.
final class ScaleUITests: PartWindowUITestCase {
    func testTypingAPercentRescalesAndResetBringsItBack() throws {
        try openFixturePart()

        let badge = element("toolbar-scale-badge")
        XCTAssertTrue(badge.waitForExistence(timeout: 10))
        badge.click()
        let field = popoverElement("scale-pct"), reset = popoverElement("scale-reset")
        XCTAssertTrue(field.waitForExistence(timeout: 5), "the popover opened")
        XCTAssertFalse(reset.isEnabled, "Reset is off at 100 %")

        field.click()
        field.typeKey("a", modifierFlags: .command)
        field.typeText("150")
        // Up and Down are swallowed: they must not step the value
        field.typeKey(.upArrow, modifierFlags: [])
        field.typeKey(.return, modifierFlags: [])
        waitFor("the badge carries the percent", timeout: 20) { self.badgeText(badge).contains("150%") }
        waitFor("Reset is on", timeout: 5) { reset.isEnabled }

        reset.click()
        waitFor("back at 100 %", timeout: 20) { !self.badgeText(badge).contains("150%") && !reset.isEnabled }
    }

    /// The badge's text ("Scale" + `#scale-now`) is its label. A button has no accessibility value, and XCUITest
    /// hands that back as a non-nil empty value, so `value as? String ?? label` read "" and never the label
    /// (`ToolbarAccessibilityTests` pins the label and the missing value).
    private func badgeText(_ badge: XCUIElement) -> String { badge.label }
}
