import XCTest

/// The object picker's "Select all" checkbox (user suggestion S1) through the real sheet: a file with three objects
/// opens the picker with the first row checked; Select all checks every row (the button reads "Merge 3 into one part"),
/// unchecking a row takes Select all's check away, checking that row again puts it back, and unchecking Select all
/// unchecks every row (Load is off). Run by the orchestrator, like the other UI tests.
///
/// Elements are found by their identifiers (`picker-select-all`, `picker-row-<file index>`) and the buttons' titles.
final class ObjectPickerUITests: PartWindowUITestCase {
    /// An OBJ of three cubes, one object each (`OpenFlowTests.threeObjects`'s file).
    private func writeThreeObjects() throws -> URL {
        var obj = ""
        var base = 1
        for (name, e) in [("small", 10), ("big", 20), ("tiny", 5)] {
            obj += "o \(name)\n"
            for z in [0, e] { for y in [0, e] { for x in [0, e] { obj += "v \(x) \(y) \(z)\n" } } }
            for f in [[1, 3, 2], [2, 3, 4], [5, 6, 7], [6, 8, 7], [1, 2, 6], [1, 6, 5], [2, 4, 8], [2, 8, 6], [4, 3, 7], [4, 7, 8], [3, 1, 5], [3, 5, 7]] {
                obj += "f " + f.map { String($0 + base - 1) }.joined(separator: " ") + "\n"
            }
            base += 8
        }
        let url = dir.appending(path: "three-objects.obj")
        try obj.write(to: url, atomically: true, encoding: .utf8)
        return url
    }

    /// An OBJ of three 10 mm cubes standing 5 mm apart along x: objects that do not overlap.
    private func writeThreeApart() throws -> URL {
        var obj = ""
        var base = 1
        for (k, name) in ["left", "middle", "right"].enumerated() {
            obj += "o \(name)\n"
            let x0 = k * 15
            for z in [0, 10] { for y in [0, 10] { for x in [x0, x0 + 10] { obj += "v \(x) \(y) \(z)\n" } } }
            for f in [[1, 3, 2], [2, 3, 4], [5, 6, 7], [6, 8, 7], [1, 2, 6], [1, 6, 5], [2, 4, 8], [2, 8, 6], [4, 3, 7], [4, 7, 8], [3, 1, 5], [3, 5, 7]] {
                obj += "f " + f.map { String($0 + base - 1) }.joined(separator: " ") + "\n"
            }
            base += 8
        }
        let url = dir.appending(path: "three-apart.obj")
        try obj.write(to: url, atomically: true, encoding: .utf8)
        return url
    }

    private func sheetElement(_ id: String) -> XCUIElement { app.sheets.descendants(matching: .any)[id].firstMatch }

    /// A checkbox's state: 1 checked, 0 unchecked (a mixed checkbox reports neither 0 nor 1).
    private func isChecked(_ e: XCUIElement) -> Bool { (e.value as? NSNumber)?.intValue == 1 }

    func testSelectAllChecksAndUnchecksEveryRow() throws {
        try openPart(writeThreeObjects())
        let all = sheetElement("picker-select-all")
        XCTAssertTrue(all.waitForExistence(timeout: 10), "the picker shows Select all")
        let rows = (0..<3).map { sheetElement("picker-row-\($0)") }
        XCTAssertTrue(rows.allSatisfy { $0.exists }, "the picker's three rows")
        XCTAssertFalse(isChecked(all), "one row of three pre-selected: Select all is not checked")

        // Select all: every row checked, the button merges three
        all.click()
        waitFor("every row checked", timeout: 5) { rows.allSatisfy { self.isChecked($0) } && self.isChecked(all) }
        XCTAssertTrue(app.sheets.buttons["Merge 3 into one part"].exists, "the button merges all three")
        XCTAssertTrue(app.sheets.buttons["Load 3 as separate models"].exists, "or loads them as models of their own")

        // one row unchecked by hand: Select all is no longer checked
        rows[1].click()
        waitFor("Select all lost its check", timeout: 5) { !self.isChecked(all) }
        XCTAssertTrue(app.sheets.buttons["Merge 2 into one part"].exists)

        // that row checked again by hand: every row is checked, so Select all is too
        rows[1].click()
        waitFor("Select all checked again", timeout: 5) { self.isChecked(all) }

        // Select all unchecked: no row checked, nothing to load
        all.click()
        waitFor("every row unchecked", timeout: 5) { !rows.contains { self.isChecked($0) } && !self.isChecked(all) }
        XCTAssertFalse(app.sheets.buttons["Load"].isEnabled, "Load is off with nothing checked")

        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(element("empty-import").waitForExistence(timeout: 10), "Cancel loaded nothing")
    }

    /// Objects that stand apart load as separate models by default: Select all, Return, three models on the plate
    /// (the toolbar's Arrange shows with the second model).
    func testObjectsThatStandApartLoadAsSeparateModels() throws {
        try openPart(writeThreeApart())
        let all = sheetElement("picker-select-all")
        XCTAssertTrue(all.waitForExistence(timeout: 10))
        all.click()
        let load = sheetElement("picker-load")
        waitFor("the default loads them separately", timeout: 5) { load.label == "Load 3 as separate models" }
        XCTAssertEqual(sheetElement("picker-load-other").label, "Merge 3 into one part")
        load.click()
        XCTAssertTrue(element("toolbar-arrange").waitForExistence(timeout: 30), "several models on the plate")
    }
}
