import XCTest

/// The Model menu (docs/UI-MAP.md 8.1): Arrange after Move, and the Rotate shortcuts.
final class MenusUITests: PartWindowUITestCase {
    func testTheModelMenuHasArrangeAndRotateShortcuts() throws {
        try openFixturePart()
        let model = menuBarItem("Model")
        model.click()
        XCTAssertTrue(model.menuItems["Arrange"].exists)
        XCTAssertTrue(model.menuItems["Rotate 90° About X"].exists)
        app.typeKey(.escape, modifierFlags: [])
    }
}
