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
        XCTAssertEqual(toggle.label, "Add fins")
        XCTAssertTrue(add.waitForExistence(timeout: 10), "the Fins row offers Add fins while they are off")
        scrollIntoView(add)
        add.click()
        let fins = element("stat-fins")
        waitFor("the Fins row shows a count", timeout: 60) { (fins.value as? String ?? fins.label).first?.isNumber == true }
        waitFor("the toggle says the fins are on", timeout: 5) { toggle.label == "Fins on" }
        XCTAssertFalse(add.exists, "the row's button goes once the fins are on")
    }
}
