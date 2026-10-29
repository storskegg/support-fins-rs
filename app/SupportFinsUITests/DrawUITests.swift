import XCTest

/// Draw through the real window (docs/UI-MAP.md rows `fin-mode`, `draw-back`, `draw-clear`, `augment-toggle`): open a
/// part, Add fins, switch Placement to Draw, look at the part from below (View > Bottom, key 6) so its red underside
/// faces the camera, click two points across it, read "1 drawn wall" in the Fins row, take a point back, clear.
///
/// Elements are found by the accessibility identifiers the app sets: `fins-toggle`, `fin-mode-draw`, `draw-hint`,
/// `draw-back`, `draw-clear`, `stat-fins`. The clicks are in the window's normalised coordinates: the inspector takes
/// the right of it, so the part's slab is left of centre. Needs the Accessibility permission for the test runner, and
/// is run by the orchestrator (`just app-uitest`), never by an agent.
final class DrawUITests: XCTestCase {
    private var app: XCUIApplication!
    private var dir: URL!

    override func setUpWithError() throws {
        continueAfterFailure = false
        dir = FileManager.default.temporaryDirectory.appending(path: "sf-uitest-draw-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        app = XCUIApplication()
        app.launchArguments = ["-ApplePersistenceIgnoreState", "YES"]
    }

    override func tearDownWithError() throws {
        app.terminate()
        try? FileManager.default.removeItem(at: dir)
    }

    func testDrawAWallAndTakeItBack() throws {
        let part = try writeFixtureSTL()
        app.launch()
        let toggle = element("fins-toggle")
        if !panelExists("open-panel", timeout: 5) && !toggle.exists { app.typeKey("o", modifierFlags: .command) }
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel")
        chooseInPanel(part.path)
        XCTAssertTrue(toggle.waitForExistence(timeout: 20), "the part's window opened")
        toggle.click()

        // Placement > Draw: the Draw controls and their hint appear
        let draw = element("fin-mode-draw")
        for _ in 0..<15 where !draw.isHittable { element("stat-fins").scroll(byDeltaX: 0, deltaY: -80) }
        XCTAssertTrue(draw.waitForExistence(timeout: 10), "the Draw button")
        draw.click()
        let hint = element("draw-hint")
        XCTAssertTrue(hint.waitForExistence(timeout: 10), "the Draw hint")
        XCTAssertTrue((hint.value as? String ?? hint.label).hasPrefix("Click two points across an overhang"))
        let fins = element("stat-fins")
        waitFor("nothing drawn yet", timeout: 30) { (fins.value as? String ?? fins.label) == "none yet" }

        // look at the part from below, then click two points across the slab's red underside
        app.typeKey("6", modifierFlags: [])
        Thread.sleep(forTimeInterval: 1)           // the camera glides 450 ms
        let window = app.windows.firstMatch
        let a = window.coordinate(withNormalizedOffset: CGVector(dx: 0.27, dy: 0.5))
        let b = window.coordinate(withNormalizedOffset: CGVector(dx: 0.45, dy: 0.5))
        a.click()
        let back = element("draw-back")
        XCTAssertTrue(back.waitForExistence(timeout: 10), "Back one point is offered while a wall is half drawn")
        back.click()
        waitFor("Back one point is gone with the point", timeout: 10) { !back.exists }
        a.click()
        XCTAssertTrue(back.waitForExistence(timeout: 10))
        b.click()
        waitFor("the Fins row counts the drawn wall", timeout: 60) { (fins.value as? String ?? fins.label).hasPrefix("1 drawn wall") }
        XCTAssertFalse(back.exists, "the wall is placed: no point to take back")

        // Clear all
        element("draw-clear").click()
        waitFor("cleared", timeout: 30) { (fins.value as? String ?? fins.label) == "none yet" }
    }

    // MARK: helpers

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

    private func chooseInPanel(_ path: String) {
        app.typeKey("g", modifierFlags: [.command, .shift])
        app.typeText(path)
        app.typeKey(.return, modifierFlags: [])
        app.typeKey(.return, modifierFlags: [])
    }

    /// A binary STL: a 10 mm stem under a 60 mm slab, so the slab's underside is an overhang to draw under.
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
