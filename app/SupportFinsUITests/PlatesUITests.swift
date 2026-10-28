import XCTest

/// Several models on one plate through the real window: open a part, Add model (the Open panel again), Remove model
/// and the toast's Undo. Elements are found by the identifiers the app sets: `add-part`, `remove-part`, `toast`,
/// `toast-undo`, `toast-text`. `PlateBoardTests` (unit tests) runs the same flow headless; like `FinsLoopUITests` this
/// needs the Mac's Accessibility permission for the test runner, and is the orchestrator's to run.
final class PlatesUITests: XCTestCase {
    private var app: XCUIApplication!
    private var dir: URL!

    override func setUpWithError() throws {
        continueAfterFailure = false
        dir = FileManager.default.temporaryDirectory.appending(path: "sf-uitest-plates-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        app = XCUIApplication()
        app.launchArguments = ["-ApplePersistenceIgnoreState", "YES"]
    }

    override func tearDownWithError() throws {
        app.terminate()
        try? FileManager.default.removeItem(at: dir)
    }

    func testAddAModelRemoveItAndUndo() throws {
        let first = try writeSTL("first.stl", slab: 30)
        let second = try writeSTL("second.stl", slab: 20)
        app.launch()

        let add = element("add-part")
        if !panelExists("open-panel", timeout: 5) && !add.exists { app.typeKey("o", modifierFlags: .command) }
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel")
        chooseInPanel(first.path)
        XCTAssertTrue(add.waitForExistence(timeout: 20), "the part's window opened, with Add model in the Part section")
        XCTAssertTrue(element("remove-part").exists)

        // Add model: the Open panel again; the file lands beside the first
        add.click()
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel for Add model")
        chooseInPanel(second.path)
        waitFor("the window title follows the new model", timeout: 20) { self.app.windows.firstMatch.title.hasPrefix("second") }

        // Remove model: the toast names it, Undo brings it back
        element("remove-part").click()
        let toast = element("toast")
        XCTAssertTrue(toast.waitForExistence(timeout: 5), "the toast")
        XCTAssertTrue((element("toast-text").value as? String ?? element("toast-text").label).hasPrefix("Removed second"))
        waitFor("the window shows the first model again", timeout: 10) { self.app.windows.firstMatch.title.hasPrefix("first") }
        element("toast-undo").click()
        waitFor("the second model is back", timeout: 10) { self.app.windows.firstMatch.title.hasPrefix("second") && !toast.exists }
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
