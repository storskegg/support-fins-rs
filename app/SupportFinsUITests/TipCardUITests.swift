import XCTest

/// The tip cards (`web/ui/tips.js`): resting on a setting shows its tooltip under its picture, moving away closes the
/// card, and Tab into a number field shows it too. The printer row of the inspector has a picture (`printer`). The launch,
/// the window and the helpers are `PartWindowUITestCase`'s.
final class TipCardUITests: PartWindowUITestCase {
    private let tooltip = "Your printer: nozzle, layer height, support gap, interface material. Click to change."
    private let gapTooltip = "Clearance between a support top and the part. Bigger = cleaner surface / easier removal; "
        + "too big stops holding the overhang. Default 0.2mm."

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

    /// Keyboard focus raises the card. With the Mac's default keyboard settings Tab visits only text fields (sliders, toggles
    /// and pickers need Keyboard navigation, which a test must not change), so the setting is a number field with a card:
    /// the Support gap, in the Printer section. A click into it shows no card by itself (the web's `byKey`): the pointer
    /// moves away, Shift-Tab leaves the field and Tab comes back to it.
    func testTabbingIntoANumberFieldShowsItsCard() throws {
        try openFixturePart()
        let row = element("printer-row")
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.click()   // unfolds the Printer section
        let gap = element("support-gap")
        XCTAssertTrue(gap.waitForExistence(timeout: 5), "the Support gap field")
        gap.click()
        element("fins-toggle").hover()   // the pointer rests on a control with no card
        waitFor("no card from the click or the pointer", timeout: 5) { !self.card.exists }

        app.typeKey(.tab, modifierFlags: .shift)   // out of the field
        app.typeKey(.tab, modifierFlags: [])       // and back in, by the keyboard
        XCTAssertTrue(card.waitForExistence(timeout: 5), "the card shows on keyboard focus")
        // a SwiftUI Text's string is the static text's value on macOS (its label is empty), so match it whole, as the
        // hover test does (`staticTexts[_]` matches the value too)
        XCTAssertTrue(card.staticTexts[gapTooltip].exists, "it is the Support gap's tooltip")
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
