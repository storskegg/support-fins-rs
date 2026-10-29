import XCTest

/// Several models on one plate through the real window: open a part, Add model (the Open panel again), Remove model
/// and the toast's Undo. Elements are found by the identifiers the app sets: `add-part`, `remove-part`, `toast`,
/// `toast-undo`, `toast-text`, and the stats card's File row `stat-name` (the model on show; the window keeps its
/// document's title). `PlateBoardTests` (unit tests) runs the same flow headless; like `FinsLoopUITests` this needs
/// the Mac's Accessibility permission for the test runner, and is the orchestrator's to run.
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

        // Add model: the Open panel again; the file lands beside the first and is the one on show. The window keeps
        // its document's name (a DocumentGroup window titles itself after the file opened); the stats card's File
        // row (`stat-name`, the web's `s-name`) names the model on show, as on the web
        let name = element("stat-name")
        XCTAssertTrue(name.waitForExistence(timeout: 10), "the File row")
        waitFor("the File row names the first model", timeout: 20) { self.text(name).hasPrefix("first") }
        add.click()
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel for Add model")
        chooseInPanel(second.path)
        waitFor("the File row follows the new model", timeout: 20) { self.text(name).hasPrefix("second") }

        // Remove model: the toast names it, Undo brings it back
        let remove = element("remove-part")
        waitFor("Remove model is enabled", timeout: 20) { remove.isEnabled }
        remove.click()
        let toast = element("toast")
        XCTAssertTrue(toast.waitForExistence(timeout: 5), "the toast")
        XCTAssertTrue(text(element("toast-text")).hasPrefix("Removed second"), "the toast names the removed model")
        waitFor("the File row shows the first model again", timeout: 10) { self.text(name).hasPrefix("first") }
        element("toast-undo").click()
        waitFor("the second model is back", timeout: 10) { self.text(name).hasPrefix("second") && !toast.exists }
    }

    /// Plates: add one, import a model into it, switch between the tabs, delete a plate and Undo. Identifiers:
    /// `plate-add`, `plate-tab-N` (value "selected" for the plate being edited), `plate-x-N`, `empty-import`,
    /// `stat-name`, `toast`, `toast-text`, `toast-undo`. No window title and no window coordinates: the File row names
    /// the model on show.
    func testAddAPlateSwitchDeleteAndUndo() throws {
        let first = try writeSTL("first.stl", slab: 30)
        let second = try writeSTL("second.stl", slab: 20)
        app.launch()
        if !panelExists("open-panel", timeout: 5) && !element("add-part").exists { app.typeKey("o", modifierFlags: .command) }
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel")
        chooseInPanel(first.path)
        let name = element("stat-name")
        XCTAssertTrue(name.waitForExistence(timeout: 20), "the part's window opened")
        waitFor("the File row names the first model", timeout: 20) { self.text(name).hasPrefix("first") }

        // + adds an empty plate, which is the one edited; the import button fills it
        let tab1 = element("plate-tab-1"), tab2 = element("plate-tab-2")
        XCTAssertTrue(tab1.waitForExistence(timeout: 5), "the first plate's tab")
        element("plate-add").click()
        XCTAssertTrue(tab2.waitForExistence(timeout: 5), "the second plate's tab")
        waitFor("the second plate is the one edited", timeout: 5) { self.text(tab2) == "selected" }
        let importButton = element("empty-import")
        XCTAssertTrue(importButton.waitForExistence(timeout: 5), "an empty plate offers Import")
        importButton.click()
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel for the new plate")
        chooseInPanel(second.path)
        waitFor("the File row follows the new plate's model", timeout: 20) { self.text(name).hasPrefix("second") }

        // a click on the first plate's tab edits it, and back
        tab1.click()
        waitFor("the first plate is edited, with its own model", timeout: 10) {
            self.text(tab1) == "selected" && self.text(name).hasPrefix("first")
        }
        tab2.click()
        waitFor("the second plate again", timeout: 10) { self.text(tab2) == "selected" && self.text(name).hasPrefix("second") }

        // × deletes the plate: the toast, then Undo brings it back
        element("plate-x-2").click()
        let toast = element("toast")
        XCTAssertTrue(toast.waitForExistence(timeout: 5), "the toast")
        XCTAssertEqual(text(element("toast-text")), "Deleted plate 2")
        waitFor("one plate is left, the first", timeout: 10) { !tab2.exists && self.text(name).hasPrefix("first") }
        element("toast-undo").click()
        waitFor("the second plate is back, with its model", timeout: 10) {
            tab2.exists && self.text(name).hasPrefix("second") && !toast.exists
        }
    }

    // MARK: helpers

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
