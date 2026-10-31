import XCTest

/// Calibration prints (`web/ui/calibrate.js`) through the real window: Help > Calibration Prints… opens the sheet
/// with one row per coupon and the app's own Plate ties print after them (issue #50), and Done closes it.
final class CalibrationUITests: PartWindowUITestCase {
    func testTheHelpMenuOpensTheListOfTestPrints() throws {
        try openFixturePart()
        let help = menuBarItem("Help")
        help.click()
        help.menuItems["Calibration Prints…"].click()

        let angle = element("calibrate-row-angle")
        XCTAssertTrue(angle.waitForExistence(timeout: 10), "the sheet lists the Angle coupon")
        for name in ["gap", "grip", "pad", "cutouts", "bore", "sampler"] { XCTAssertTrue(element("calibrate-row-\(name)").exists, name) }
        XCTAssertTrue(element("calibrate-download-angle").exists)
        XCTAssertTrue(element("calibrate-profile").exists, "the footer names the profile the estimates came from")
        // the optional Plate ties print: listed last, made for the printer picked, so its Download is live
        XCTAssertTrue(element("calibrate-row-plate-ties").exists, "the sheet lists the Plate ties print")
        let ties = element("calibrate-download-plate-ties")
        XCTAssertTrue(ties.exists)
        XCTAssertTrue(ties.isEnabled, "the window's printer and settings give the print its thicknesses")

        element("calibrate-done").click()
        waitFor("the sheet closed", timeout: 5) { !angle.exists }
    }
}
