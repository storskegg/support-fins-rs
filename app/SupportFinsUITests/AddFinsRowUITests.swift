import XCTest

/// S2 (November 2026): the Fins row offers Add fins while the fins are off, and the toolbar's toggle carries its words.
/// `AddFinsFoundTests` (unit tests) covers the same headless.
final class AddFinsRowUITests: PartWindowUITestCase {
    func testTheFinsRowAddsTheFins() throws {
        try openFixturePart()
        let toggle = element("fins-toggle")
        XCTAssertTrue(toggle.waitForExistence(timeout: 10))
        let add = element("stat-add-fins")
        if toggle.label == "Fins on" {
            // a fixture that opens with the fins on: off first, so the row offers them
            toggle.click()
            XCTAssertTrue(add.waitForExistence(timeout: 10), "the Fins row offers Add fins while they are off")
        }
        XCTAssertEqual(toggle.label, "Add fins", "the toggle's words while the fins are off (its value \(String(describing: toggle.value)))")
        XCTAssertTrue(add.waitForExistence(timeout: 10), "the Fins row offers Add fins while they are off")
        // pinned above the inspector's tabs: on show in any tab, with nothing to scroll
        showInspectorTab(.more)
        XCTAssertTrue(add.isHittable, "Add fins is on show in the More tab too")
        add.click()
        let fins = element("stat-fins")
        waitFor("the Fins row shows a count", timeout: 60) { (fins.value as? String ?? fins.label).first?.isNumber == true }
        // on: the switch's state (its value, 1) and its words. The UI run of 2026-11-01 failed "the toggle says the fins are
        // on" with no word of what it said, while its recording showed "Fins on" in the toolbar; in-process the label
        // reads "Fins on" in a real document window (`DocumentWindowToolbarTests`). A failure says both now
        let state = { "the toggle reads \"\(self.element("fins-toggle").label)\", its value \(String(describing: self.element("fins-toggle").value)), the Fins row \"\(self.text(fins))\"" }
        if !waitUntil(timeout: 5, { (self.element("fins-toggle").value as? Int) == 1 }) {
            XCTFail("the toggle is on: \(state())")
        }
        if !waitUntil(timeout: 5, { self.element("fins-toggle").label == "Fins on" }) {
            XCTFail("the toggle says the fins are on: \(state())")
        }
        XCTAssertFalse(add.exists, "the row's button goes once the fins are on")
    }
}
