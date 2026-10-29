import XCTest

/// Move through the real window: open a part, then turn the tool on and off with the toolbar's Move badge, the M
/// key, Esc and Model > Move. The badge says whether the tool is on by being selected. A drag
/// across the plate needs window coordinates, so it is left to the headless `MoveModelTests`. Elements are found by
/// identifier (`toolbar-move`, `fins-toggle`); the menu bar only through `app.menuBars`.
final class MoveUITests: XCTestCase {
    private var app: XCUIApplication!
    private var dir: URL!

    override func setUpWithError() throws {
        continueAfterFailure = false
        dir = FileManager.default.temporaryDirectory.appending(path: "sf-uitest-move-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        app = XCUIApplication()
        app.launchArguments = ["-ApplePersistenceIgnoreState", "YES"]
    }

    override func tearDownWithError() throws {
        app.terminate()
        try? FileManager.default.removeItem(at: dir)
    }

    func testMoveTurnsOnAndOffFromTheBadgeTheKeyEscAndTheMenu() throws {
        let part = try writeSTL("bracket.stl", slab: 30)
        app.launch()
        let toggle = element("fins-toggle")
        if !panelExists("open-panel", timeout: 5) && !toggle.exists { app.typeKey("o", modifierFlags: .command) }
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel")
        chooseInPanel(part.path)
        XCTAssertTrue(toggle.waitForExistence(timeout: 20), "the part's window opened")

        let badge = element("toolbar-move")
        XCTAssertTrue(badge.waitForExistence(timeout: 10), "the Move badge")
        XCTAssertEqual(state(badge), "off")

        badge.click()
        waitFor("the badge turns Move on", timeout: 5) { self.state(badge) == "on" }
        badge.click()
        waitFor("the badge turns it off", timeout: 5) { self.state(badge) == "off" }

        // M toggles it, Esc leaves it
        app.typeKey("m", modifierFlags: [])
        waitFor("M turns it on", timeout: 5) { self.state(badge) == "on" }
        app.typeKey(.escape, modifierFlags: [])
        waitFor("Esc turns it off", timeout: 5) { self.state(badge) == "off" }

        // Model > Move
        app.menuBars.menuBarItems["Model"].click()
        app.menuBars.menuItems["Move"].click()
        waitFor("the menu turns it on", timeout: 5) { self.state(badge) == "on" }
        app.menuBars.menuBarItems["Model"].click()
        app.menuBars.menuItems["Move"].click()
        waitFor("the menu turns it off", timeout: 5) { self.state(badge) == "off" }
    }

    // MARK: helpers

    /// "on" while the badge is selected (its tool is on), else "off".
    private func state(_ e: XCUIElement) -> String { e.isSelected ? "on" : "off" }

    private func element(_ id: String) -> XCUIElement { app.descendants(matching: .any)[id].firstMatch }

    /// A static text's string: its value, or its label when the value is empty.
    private func text(_ e: XCUIElement) -> String {
        guard e.exists else { return "" }
        if let v = e.value as? String, !v.isEmpty { return v }
        return e.label
    }

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

    /// In an Open panel: ⇧⌘G, the path, Return (go to it), Return (Open).
    private func chooseInPanel(_ path: String) {
        app.typeKey("g", modifierFlags: [.command, .shift])
        app.typeText(path)
        app.typeKey(.return, modifierFlags: [])
        app.typeKey(.return, modifierFlags: [])
    }

    /// A binary STL: a 10 mm stem under a `slab` mm (half-width) slab, so there is an overhang to put fins under.
    private func writeSTL(_ name: String, slab: Float) throws -> URL {
        func box(_ x0: Float, _ x1: Float, _ y0: Float, _ y1: Float, _ z0: Float, _ z1: Float) -> [[Float]] {
            let v: [[Float]] = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]]
            let faces = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]]
            return faces.map { f in f.flatMap { v[$0] } }
        }
        let tris = box(-5, 5, -5, 5, 0, 20) + box(-slab, slab, -slab, slab, 20, 24)
        var data = Data(count: 80)
        var count = UInt32(tris.count)
        withUnsafeBytes(of: &count) { data.append(contentsOf: $0) }
        for t in tris {
            for f in [Float](repeating: 0, count: 3) + t { var x = f; withUnsafeBytes(of: &x) { data.append(contentsOf: $0) } }
            data.append(contentsOf: [0, 0])
        }
        let url = dir.appending(path: name)
        try data.write(to: url)
        return url
    }
}
