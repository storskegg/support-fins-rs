import XCTest

/// The tip cards (`web/ui/tips.js`): resting on a setting shows its tooltip under its picture, moving away closes the
/// card, and Tab into a setting shows it too. The printer row of the inspector has a picture (`printer`). The launch,
/// the window and the helpers are `PartWindowUITestCase`'s.
final class TipCardUITests: PartWindowUITestCase {
    private let tooltip = "Your printer: nozzle, layer height, support gap, interface material. Click to change."

    private var card: XCUIElement { popoverElement("tip-card") }

    /// Rest the pointer on the printer row until the card is up (it waits 350 ms).
    private func restOnPrinterRow() {
        let row = element("printer-row")
        XCTAssertTrue(row.waitForExistence(timeout: 10), "the printer row")
        row.hover()
    }

    func testHoverShowsTheCardAndLeavingClosesIt() throws {
        try openFixturePart()
        restOnPrinterRow()
        XCTAssertTrue(card.waitForExistence(timeout: 5), "the card appears after the pointer rests on the setting")
        XCTAssertTrue(card.staticTexts[tooltip].exists, "it carries the setting's tooltip")
        // away from every setting
        element("fins-toggle").hover()
        waitFor("the card closes", timeout: 5) { !self.card.exists }
    }

    func testTabIntoASettingShowsTheCard() throws {
        try openFixturePart()
        XCTAssertTrue(element("printer-row").waitForExistence(timeout: 10))
        element("fins-toggle").hover()   // the pointer rests on a control with no card
        var tabs = 0
        while !card.exists && tabs < 30 {
            app.typeKey(.tab, modifierFlags: [])
            tabs += 1
            _ = card.waitForExistence(timeout: 0.5)
        }
        XCTAssertTrue(card.exists, "Tab reached a setting with a card (\(tabs) presses)")
        XCTAssertFalse(card.staticTexts.allElementsBoundByIndex.filter { !$0.label.isEmpty }.isEmpty, "the card has its tooltip")
    }
}

/// The same card in Arabic, with the layout mirrored: the text is the web's Arabic tooltip.
final class TipCardArabicUITests: PartWindowUITestCase {
    func testTheCardShowsTheArabicTooltip() throws {
        app.launchArguments += ["-AppleLanguages", "(ar)", "-AppleLocale", "ar_SA", "-AppleTextDirection", "YES"]
        try openFixturePart()
        let row = element("printer-row")
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.hover()
        let card = popoverElement("tip-card")
        XCTAssertTrue(card.waitForExistence(timeout: 5), "the card appears")
        XCTAssertTrue(card.staticTexts["طابعتك: الفوهة، ارتفاع الطبقة، فجوة الدعم، مادة التلامس. انقر للتغيير."].exists,
                      "its text is the Arabic tooltip")
    }
}
