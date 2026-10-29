import XCTest

/// The M4b loop through the real window: open a part through the app's document path (the Open panel), turn Add
/// fins on, read the Fins row, change a setting and wait for the rebuild, export an STL to a temp directory.
/// `EndToEndTests` (unit tests) runs the same loop headless, so it is covered where these cannot run.
///
/// Elements are found by the accessibility identifiers the app sets: `fins-toggle`, `stat-fins`, `tine-density`,
/// `build-spinner`, `export-menu`. The launch, the window and the helpers are `PartWindowUITestCase`'s.
final class FinsLoopUITests: PartWindowUITestCase {
    func testOpenBuildChangeAndExport() throws {
        try openFixturePart()

        // Add fins -> the build -> the Fins row shows a count
        element(Self.windowMark).click()
        let fins = element("stat-fins")
        XCTAssertTrue(fins.waitForExistence(timeout: 10))
        waitFor("the Fins row shows a count", timeout: 60) { (fins.value as? String ?? fins.label).first?.isNumber == true }

        // change a setting the engine takes -> the rebuild lands (the spinner shows after 300 ms, only on a slow one)
        let slider = element("tine-density")
        XCTAssertTrue(slider.waitForExistence(timeout: 5))
        let before = String(describing: slider.value)   // a number or a string, depending on the control
        // the build's Status block pushes the slider below the inspector's fold. adjust(toNormalizedSliderPosition:)
        // leaves a SwiftUI slider on macOS where it was; a click on the track moves it
        scrollIntoView(slider)
        slider.coordinate(withNormalizedOffset: CGVector(dx: 0.6, dy: 0.5)).click()
        XCTAssertNotEqual(String(describing: slider.value), before, "the slider moved")
        Thread.sleep(forTimeInterval: 1)           // the 180 ms debounce, then the build
        waitFor("the rebuild is done", timeout: 60) { !self.element("build-spinner").exists }
        waitFor("the Fins row still shows a count", timeout: 5) { (fins.value as? String ?? fins.label).first?.isNumber == true }

        // Export > STL -> the save panel -> a file in the temp dir
        element("export-menu").click()
        // "STL" is also under File > Export: click the one in the menu just opened
        let stl = app.menuItems.matching(identifier: "STL").allElementsBoundByIndex.first { $0.isHittable }
        try XCTUnwrap(stl, "the export menu's STL item").click()
        XCTAssertTrue(panelExists("save-panel", timeout: 10), "the Save panel")
        saveInPanel(into: dir)
        // the export runs after the panel closes: wait for the file rather than look once
        let partName = try XCTUnwrap(part).lastPathComponent
        let file = waitForFile(in: dir, timeout: 30, minSize: 84) { $0.pathExtension == "stl" && $0.lastPathComponent != partName }
        XCTAssertNotNil(file, "an STL of more than 84 bytes was written to \(dir.path)")
    }

    /// One menu of each title: the View features' items go into the system's View menu, not a second one.
    func testTheMenuBarHasOneMenuOfEachTitle() throws {
        try openFixturePart()
        for title in ["File", "Edit", "View", "Model", "Window", "Help"] { _ = menuBarItem(title) }
    }
}
