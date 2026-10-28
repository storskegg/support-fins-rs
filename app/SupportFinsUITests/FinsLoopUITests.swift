import XCTest

/// The M4b loop through the real window: open a part through the app's document path (the Open panel), turn Add
/// fins on, read the Fins row, change a setting and wait for the rebuild, export an STL to a temp directory.
/// `EndToEndTests` (unit tests) runs the same loop headless, so it is covered where these cannot run: XCUITest
/// needs the Mac's Accessibility permission for the test runner ("Timed out while enabling automation mode"
/// otherwise).
///
/// Elements are found by the accessibility identifiers the app sets: `fins-toggle`, `stat-fins`, `tine-density`,
/// `build-spinner`, `export-menu`.
final class FinsLoopUITests: XCTestCase {
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

    func testOpenBuildChangeAndExport() throws {
        let part = try writeFixtureSTL()
        app.launch()

        // open the part: the document app's Open panel (shown at launch, or ⌘O), the path typed with ⇧⌘G
        let toggle = element("fins-toggle")
        if !panelExists("open-panel", timeout: 5) && !toggle.exists { app.typeKey("o", modifierFlags: .command) }
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel")
        chooseInPanel(part.path)
        XCTAssertTrue(toggle.waitForExistence(timeout: 20), "the part's window opened")

        // Add fins -> the build -> the Fins row shows a count
        toggle.click()
        let fins = element("stat-fins")
        XCTAssertTrue(fins.waitForExistence(timeout: 10))
        waitFor("the Fins row shows a count", timeout: 60) { (fins.value as? String ?? fins.label).first?.isNumber == true }

        // change a setting the engine takes -> the rebuild lands (the spinner shows after 300 ms, only on a slow one)
        let slider = element("tine-density")
        XCTAssertTrue(slider.waitForExistence(timeout: 5))
        let before = String(describing: slider.value)   // a number or a string, depending on the control
        // the build's Status block pushes the slider below the inspector's fold: scroll it into view before clicking.
        // adjust(toNormalizedSliderPosition:) leaves a SwiftUI slider on macOS where it was; a click on the track moves it
        for _ in 0..<15 where !slider.isHittable { fins.scroll(byDeltaX: 0, deltaY: -80) }
        XCTAssertTrue(slider.isHittable, "the slider scrolled into view")
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
        let exported = try FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: [.fileSizeKey])
            .filter { $0.pathExtension == "stl" && $0.lastPathComponent != part.lastPathComponent }
        let file = try XCTUnwrap(exported.first, "an STL was written to \(dir.path)")
        let size = try XCTUnwrap(try file.resourceValues(forKeys: [.fileSizeKey]).fileSize)
        XCTAssertGreaterThan(size, 84, "\(file.lastPathComponent) is not empty")
    }

    // MARK: helpers

    private func element(_ id: String) -> XCUIElement { app.descendants(matching: .any)[id].firstMatch }

    private func waitFor(_ what: String, timeout: TimeInterval, _ condition: @escaping () -> Bool) {
        let e = expectation(for: NSPredicate { _, _ in condition() }, evaluatedWith: nil)
        XCTAssertEqual(XCTWaiter().wait(for: [e], timeout: timeout), .completed, what)
    }

    /// An Open or Save panel: a dialog or sheet, or (macOS 26) a window with the panel's identifier.
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

    /// In a Save panel: ⇧⌘G to the directory (the suggested file name stays), then Return (Save).
    private func saveInPanel(into dir: URL) {
        app.typeKey("g", modifierFlags: [.command, .shift])
        app.typeText(dir.path)
        app.typeKey(.return, modifierFlags: [])
        app.typeKey(.return, modifierFlags: [])
    }

    /// A binary STL: a 10 mm stem under a 60 mm slab, so there is an overhang to put fins under.
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
