import XCTest

/// The banner after a run that did not end normally (1D). `-SFPlantCrash YES` (a DEBUG-only hook of the app) plants a
/// "running" marker in the test's own crash folder, as if the last run had been killed; the app finds it at launch.
final class CrashBannerUITests: PartWindowUITestCase {
    func testTheBannerAppearsAfterAPlantedMarkerAndDismissHidesIt() throws {
        app.launchArguments += ["-SFPlantCrash", "YES"]
        try openFixturePart()
        let message = element("crash-banner-message")
        let shown = message.waitForExistence(timeout: 10)
        if !shown {
            let a = XCTAttachment(string: app.debugDescription)
            a.name = "app.debugDescription"
            a.lifetime = .keepAlways
            add(a)
        }
        XCTAssertTrue(shown, "the banner shows after a run that did not end normally")
        XCTAssertEqual(text(message), "Support Fins closed unexpectedly last time.")
        XCTAssertTrue(element("crash-banner-show").exists)
        XCTAssertTrue(element("crash-banner-save").exists)

        // Help > Show crash reports sits under Report a problem
        let help = menuBarItem("Help")
        help.click()
        XCTAssertTrue(help.menuItems["Show crash reports"].exists)
        app.typeKey(.escape, modifierFlags: [])

        element("crash-banner-dismiss").click()
        waitFor("the banner is gone", timeout: 5) { !message.exists }
    }

    func testNoBannerAfterANormalLaunch() throws {
        try openFixturePart()
        XCTAssertFalse(element("crash-banner-message").exists)
    }
}
