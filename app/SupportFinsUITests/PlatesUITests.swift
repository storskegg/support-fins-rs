import XCTest

/// Several models on one plate through the real window: open a part, Add model (the Open panel again), Remove model
/// and the toast's Undo. Elements are found by the identifiers the app sets: `add-part`, `remove-part`, `toast`,
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
}
