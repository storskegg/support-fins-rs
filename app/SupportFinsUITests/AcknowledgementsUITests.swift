import XCTest

/// Support Fins ▸ Acknowledgements… opens the window of open-source notices: titled "Acknowledgements", its text naming
/// OpenCascade and its LGPL licence (the relinking note), a window of the app apart from the part's.
final class AcknowledgementsUITests: PartWindowUITestCase {
    func testTheAppMenuOpensTheAcknowledgements() throws {
        try openFixturePart()
        let menu = menuBarItem("Support Fins")
        menu.click()
        let item = menu.menuItems["Acknowledgements…"]
        XCTAssertTrue(item.waitForExistence(timeout: 5), "the app menu's Acknowledgements… item")
        item.click()
        let window = app.windows["Acknowledgements"]
        XCTAssertTrue(window.waitForExistence(timeout: 5), "the Acknowledgements window")
        let text = window.descendants(matching: .any)["acknowledgements-text"].firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 5), "the notices' text view")
        let value = text.value as? String ?? ""
        XCTAssertTrue(value.contains("OpenCascade"), "credits OpenCascade")
        XCTAssertTrue(value.contains("LGPL"), "names its LGPL licence")
        window.buttons[XCUIIdentifierCloseWindow].click()
    }
}
