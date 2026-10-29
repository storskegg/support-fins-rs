import XCTest

/// Calibration prints (`web/ui/calibrate.js`) through the real window: Help > Calibration Prints… opens the sheet
/// with one row per coupon, a row unfolds to what it sets, and Done closes it.
final class CalibrationUITests: PartWindowUITestCase {
    func testTheHelpMenuOpensTheListOfTestPrints() throws {
        try openFixturePart()
        menuBarItem("Help").click()
        app.menuItems["Calibration Prints…"].click()

        let angle = element("calibrate-row-angle")
        XCTAssertTrue(angle.waitForExistence(timeout: 10), "the sheet lists the Angle coupon")
        for name in ["gap", "grip", "pad", "cutouts", "bore", "sampler"] { XCTAssertTrue(element("calibrate-row-\(name)").exists, name) }
        XCTAssertTrue(element("calibrate-download-angle").exists)
        XCTAssertTrue(element("calibrate-profile").exists, "the footer names the profile the estimates came from")

        element("calibrate-done").click()
        waitFor("the sheet closed", timeout: 5) { !angle.exists }
    }
}
