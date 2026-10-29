import XCTest

/// Several models on one plate through the real window: open a part, Add model (the Open panel again), Remove model
/// and the toast's Undo; then plates: add one (`plate-add`, `empty-import`), switch by tab (`plate-tab-N`), delete
/// (`plate-x-N`) and Undo. Elements are found by the identifiers the app sets: `add-part`, `remove-part`, `toast`,
/// `toast-undo`, `toast-text`, and the stats card's File row `stat-name` (the model on show; the window keeps its
/// document's title). `PlateBoardTests` (unit tests) runs the same flow headless; like `FinsLoopUITests` this needs
/// the Mac's Accessibility permission for the test runner, and is the orchestrator's to run. The launch, the window
/// and the helpers are `PartWindowUITestCase`'s.
final class PlatesUITests: PartWindowUITestCase {
    func testAddAModelRemoveItAndUndo() throws {
        let first = try writeSTL("first.stl", slab: 30)
        let second = try writeSTL("second.stl", slab: 20)
        try openPart(first)

        let add = element("add-part")
        XCTAssertTrue(add.waitForExistence(timeout: 20), "Add model in the Part section")
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

    func testAddAPlateSwitchDeleteAndUndo() throws {
        let first = try writeSTL("first.stl", slab: 30)
        let second = try writeSTL("second.stl", slab: 20)
        try openPart(first)
        let name = element("stat-name")
        XCTAssertTrue(name.waitForExistence(timeout: 20), "the File row")
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

    /// Export > All plates (3MF): two plates, the menu item, the Save panel, a .zip in the temp directory.
    func testExportAllPlatesWritesAZip() throws {
        let first = try writeSTL("first.stl", slab: 30)
        let second = try writeSTL("second.stl", slab: 20)
        try openPart(first)
        let name = element("stat-name")
        waitFor("the File row names the first model", timeout: 20) { self.text(name).hasPrefix("first") }
        element("plate-add").click()
        let importButton = element("empty-import")
        XCTAssertTrue(importButton.waitForExistence(timeout: 5), "an empty plate offers Import")
        importButton.click()
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel for the new plate")
        chooseInPanel(second.path)
        waitFor("the File row follows the new plate's model", timeout: 20) { self.text(name).hasPrefix("second") }

        element("export-menu").click()
        let all = app.menuItems.matching(identifier: "All plates (3MF)").allElementsBoundByIndex.first { $0.isHittable }
        try XCTUnwrap(all, "the export menu's All plates item").click()
        XCTAssertTrue(panelExists("save-panel", timeout: 10), "the Save panel")
        saveInPanel(into: dir)
        XCTAssertNotNil(waitForFile(in: dir, timeout: 30, minSize: 100) { $0.pathExtension == "zip" }, "the zip is written")
    }
}
