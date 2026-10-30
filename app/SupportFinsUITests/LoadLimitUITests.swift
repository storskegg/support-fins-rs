import XCTest

/// "Load anyway?" through the real window (docs/FFI.md "Load limits"): a part 3 m wide asks before it loads. Return or
/// Esc is Cancel (the default) and loads nothing; Load anyway loads it. Run by the orchestrator, like the other UI
/// tests (it drives the Open panel); the launch and the window are `PartWindowUITestCase`'s.
///
/// Elements are found by their identifiers (`empty-import`, `stat-name`) and the alert's button titles.
final class LoadLimitUITests: PartWindowUITestCase {
    /// A binary STL of a 3000 mm cube: 12 triangles, over the 2 m limit.
    private func writeHugeSTL() throws -> URL {
        let url = dir.appending(path: "huge-cube.stl")
        let v: [[Float]] = [[0, 0, 0], [3000, 0, 0], [3000, 3000, 0], [0, 3000, 0], [0, 0, 3000], [3000, 0, 3000], [3000, 3000, 3000], [0, 3000, 3000]]
        let faces = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]]
        var data = Data(count: 80)
        var count = UInt32(faces.count)
        withUnsafeBytes(of: &count) { data.append(contentsOf: $0) }
        for f in faces {
            for x in [Float](repeating: 0, count: 3) + f.flatMap({ v[$0] }) { var y = x; withUnsafeBytes(of: &y) { data.append(contentsOf: $0) } }
            data.append(contentsOf: [0, 0])
        }
        try data.write(to: url)
        return url
    }

    private func alertButton(_ title: String) -> XCUIElement {
        let inSheet = app.sheets.buttons[title].firstMatch, inDialog = app.dialogs.buttons[title].firstMatch
        _ = waitUntil(timeout: 20) { inSheet.exists || inDialog.exists }
        return inSheet.exists ? inSheet : inDialog
    }

    func testCancelIsTheDefaultAndLoadAnywayLoads() throws {
        let huge = try writeHugeSTL()
        try openPart(huge)

        // the prompt names the model's size and the limit; Return is Cancel
        XCTAssertTrue(alertButton("Cancel").exists, "the prompt has a Cancel button")
        XCTAssertTrue(alertButton("Load anyway").exists, "and a Load anyway button")
        XCTAssertTrue(app.staticTexts["This model is 3 m across (the limit is 2 m)."].exists, "the message states the size and the limit")
        app.typeKey(.return, modifierFlags: [])
        XCTAssertTrue(element("empty-import").waitForExistence(timeout: 10), "Cancel loaded nothing: the window is empty")

        // again: Load anyway
        element("empty-import").click()
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel")
        chooseInPanel(huge.path)
        alertButton("Load anyway").click()
        XCTAssertTrue(element("stat-name").waitForExistence(timeout: 20), "Load anyway loaded the part")
        XCTAssertFalse(element("empty-import").exists)
    }

    func testEscapeCancels() throws {
        try openPart(writeHugeSTL())
        XCTAssertTrue(alertButton("Load anyway").exists)
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(element("empty-import").waitForExistence(timeout: 10), "Esc is Cancel: nothing loaded")
    }
}
