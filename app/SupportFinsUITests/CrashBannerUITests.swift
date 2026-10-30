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

        // Dismiss before the Help menu is opened: a click that arrives while a menu is still closing (just after Esc)
        // is taken by the menu's tracking, not the button. The app is activated and the part window made key first
        // (a click on its title bar, clear of the toolbar), so this checks Dismiss itself; the first click in a window
        // that is not key is the app test `theBannerTakesTheFirstClickInAnInactiveWindow`.
        app.activate()
        partWindow.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0)).withOffset(CGVector(dx: 0, dy: 12)).click()
        Thread.sleep(forTimeInterval: 0.5)
        attachState("before Dismiss")
        let dismiss = element("crash-banner-dismiss")
        let hittable = dismiss.isHittable
        XCTAssertTrue(hittable, "Dismiss can be clicked")
        dismiss.click()
        attachState("just after Dismiss")
        let gone = waitUntil(timeout: 5) { !message.exists }
        if !gone { attachState("5 s after Dismiss") }
        XCTAssertTrue(gone, "the banner is gone")

        // Help > Show crash reports sits under Report a problem
        let help = menuBarItem("Help")
        help.click()
        XCTAssertTrue(help.menuItems["Show crash reports"].exists)
        app.typeKey(.escape, modifierFlags: [])
    }

    /// The app's windows, the whole tree and a screenshot, kept with the result (to tell a click that missed from a
    /// banner that came back, or a second window with a banner of its own). The crash folder is
    /// `~/Library/Containers/com.storskegg.SupportFins/Data/tmp/sf-crash-tests/<this test's dir name>`: a `seen.marker`
    /// there means Dismiss ran.
    private func attachState(_ when: String) {
        let windows = app.windows.allElementsBoundByIndex.map { "\($0.identifier) '\($0.title)' \($0.frame)" }
        let banners = app.descendants(matching: .any).matching(identifier: "crash-banner-message").count
        let info = "crash folder name: \(dir.lastPathComponent)\nwindows (\(windows.count)):\n\(windows.joined(separator: "\n"))\n" +
            "crash-banner-message elements in the app: \(banners)\n\n\(app.debugDescription)"
        let text = XCTAttachment(string: info)
        text.name = "\(when): tree"
        text.lifetime = .keepAlways
        add(text)
        let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        shot.name = "\(when): screen"
        shot.lifetime = .keepAlways
        add(shot)
    }

    func testNoBannerAfterANormalLaunch() throws {
        try openFixturePart()
        XCTAssertFalse(element("crash-banner-message").exists)
    }
}
