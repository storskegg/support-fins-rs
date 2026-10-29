import XCTest

/// Scale through the real window (`web/ui/scale.js`): the Scale badge opens the popover, a typed percent plus
/// Return reloads the part and shows the percent on the badge, Reset (enabled only while scaled) brings it back.
/// Self-contained: it opens the part the way `FinsLoopUITests` does.
final class ScaleUITests: XCTestCase {
    private var app: XCUIApplication!
    private var dir: URL!

    override func setUpWithError() throws {
        continueAfterFailure = false
        dir = FileManager.default.temporaryDirectory.appending(path: "sf-uitest-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        app = XCUIApplication()
        app.launchArguments = ["-ApplePersistenceIgnoreState", "YES"]
    }

    override func tearDownWithError() throws {
        app.terminate()
        try? FileManager.default.removeItem(at: dir)
    }

    func testTypingAPercentRescalesAndResetBringsItBack() throws {
        let part = try writeFixtureSTL()
        app.launch()
        let toggle = element("fins-toggle")
        if !panelExists("open-panel", timeout: 5) && !toggle.exists { app.typeKey("o", modifierFlags: .command) }
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel")
        app.typeKey("g", modifierFlags: [.command, .shift])
        app.typeText(part.path)
        app.typeKey(.return, modifierFlags: [])
        app.typeKey(.return, modifierFlags: [])
        XCTAssertTrue(toggle.waitForExistence(timeout: 20), "the part's window opened")

        let badge = app.buttons["toolbar-scale-badge"].firstMatch
        XCTAssertTrue(badge.waitForExistence(timeout: 10))
        badge.click()
        let field = element("scale-pct"), reset = element("scale-reset")
        XCTAssertTrue(field.waitForExistence(timeout: 5), "the popover opened")
        XCTAssertFalse(reset.isEnabled, "Reset is off at 100 %")

        field.click()
        field.typeKey("a", modifierFlags: .command)
        field.typeText("150")
        // Up and Down are swallowed: they must not step the value
        field.typeKey(.upArrow, modifierFlags: [])
        field.typeKey(.return, modifierFlags: [])
        waitFor("the badge carries the percent", timeout: 20) { self.badgeText(badge).contains("150%") }
        waitFor("Reset is on", timeout: 5) { reset.isEnabled }

        reset.click()
        waitFor("back at 100 %", timeout: 20) { !self.badgeText(badge).contains("150%") && !reset.isEnabled }
    }

    // MARK: helpers

    /// The badge's text ("Scale" + `#scale-now`) is its label. A button has no accessibility value, and XCUITest
    /// hands that back as a non-nil empty value, so `value as? String ?? label` read "" and never the label
    /// (`ToolbarAccessibilityTests` pins the label and the missing value).
    private func badgeText(_ badge: XCUIElement) -> String { badge.label }
    private func element(_ id: String) -> XCUIElement { app.descendants(matching: .any)[id].firstMatch }

    private func waitFor(_ what: String, timeout: TimeInterval, _ condition: @escaping () -> Bool) {
        let e = expectation(for: NSPredicate { _, _ in condition() }, evaluatedWith: nil)
        XCTAssertEqual(XCTWaiter().wait(for: [e], timeout: timeout), .completed, what)
    }

    private func panelExists(_ id: String, timeout: TimeInterval) -> Bool {
        let shown = NSPredicate { _, _ in
            self.app.dialogs.firstMatch.exists || self.app.sheets.firstMatch.exists || self.app.windows[id].exists
        }
        return XCTWaiter().wait(for: [XCTNSPredicateExpectation(predicate: shown, object: nil)], timeout: timeout) == .completed
    }

    private func writeFixtureSTL() throws -> URL {
        func box(_ x0: Float, _ x1: Float, _ y0: Float, _ y1: Float, _ z0: Float, _ z1: Float) -> [[Float]] {
            let v: [[Float]] = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]]
            let faces = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]]
            return faces.map { f in f.flatMap { v[$0] } }
        }
        let tris = box(-5, 5, -5, 5, 0, 20) + box(-30, 30, -30, 30, 20, 24)
        var data = Data(count: 80)
        var count = UInt32(tris.count)
        withUnsafeBytes(of: &count) { data.append(contentsOf: $0) }
        for t in tris {
            for f in [Float](repeating: 0, count: 3) + t { var x = f; withUnsafeBytes(of: &x) { data.append(contentsOf: $0) } }
            data.append(contentsOf: [0, 0])
        }
        let url = dir.appending(path: "bracket.stl")
        try data.write(to: url)
        return url
    }
}
