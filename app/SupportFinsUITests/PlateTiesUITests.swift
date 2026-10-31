import XCTest

/// Plate ties through the real window (issue #50, slice 2): two upright coins on a plate (the first opened, the second
/// by Add model, which stands it about 10 mm beside the first), Max gap raised to 15 mm so they are neighbours, then
/// the "Plate ties" switch (`plate-ties-toggle`). The link shows in the section (`plate-tie-link-0`, the same switch a
/// click on the link in the view is) and the status card says what was tied; Arrange is greyed while the ties are on;
/// the plate's 3MF is one object. `PlateTiesTests` runs the same flows headless, the click on a link in the view too.
final class PlateTiesUITests: PartWindowUITestCase {
    /// An upright coin (r 20, 3 mm thick, its faces along x) as a binary STL in the test's directory.
    func writeCoin(_ name: String) throws -> URL {
        let n = 96, r: Float = 20, h: Float = 1.5
        func p(_ x: Float, _ i: Int) -> [Float] {
            let a = Float(i % n) / Float(n) * 2 * .pi
            return [x, r * cos(a), r + r * sin(a)]
        }
        let c = { (x: Float) -> [Float] in [x, 0, r] }
        var tris: [[Float]] = []
        for i in 0..<n {
            tris.append(c(h) + p(h, i) + p(h, i + 1))
            tris.append(c(-h) + p(-h, i + 1) + p(-h, i))
            tris.append(p(-h, i) + p(-h, i + 1) + p(h, i + 1))
            tris.append(p(-h, i) + p(h, i + 1) + p(h, i))
        }
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

    /// The status card's lines.
    var statusNotes: [String] {
        partWindow.descendants(matching: .any).matching(identifier: "status-note").allElementsBoundByIndex.map { text($0) }
    }

    /// Two coins on the plate, the ties on with Max gap 15 mm: one link, three ties.
    func tiedCoins() throws {
        try openPart(try writeCoin("coin-a.stl"))
        let name = element("stat-name")
        waitFor("the File row names the first coin", timeout: 20) { self.text(name).hasPrefix("coin-a") }
        element("add-part").click()
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel for Add model")
        chooseInPanel(try writeCoin("coin-b.stl").path)
        waitFor("the second coin is on the plate", timeout: 20) { self.text(name).hasPrefix("coin-b") }

        let toggle = element("plate-ties-toggle")
        XCTAssertTrue(toggle.waitForExistence(timeout: 10), "the Plate ties switch")
        scrollIntoView(toggle)
        toggle.click()
        // Add model leaves 10 mm between them: Max gap 15 makes them neighbours
        let gap = element("plate-ties-max-gap")
        XCTAssertTrue(gap.waitForExistence(timeout: 10), "Max gap shows once the ties are on")
        scrollIntoView(gap)
        gap.doubleClick()
        gap.typeKey("a", modifierFlags: .command)
        gap.typeText("15\r")
        waitFor("the coins are tied: one link, three ties", timeout: 30) {
            self.statusNotes.contains { $0.hasPrefix("Plate ties: 3 of 3 ties on 1 pairs") }
        }
    }

    func testTurningTiesOnShowsTheLink() throws {
        try tiedCoins()
        let link = element("plate-tie-link-0")
        XCTAssertTrue(link.waitForExistence(timeout: 5), "the link is listed")
        XCTAssertFalse(element("plate-tie-link-1").exists, "one pair")
        XCTAssertTrue(statusNotes.contains { $0.hasPrefix("Plate ties are on: Arrange and Move are locked") }, "\(statusNotes)")
    }

    func testAClickedLinkTurnsOff() throws {
        try tiedCoins()
        let link = element("plate-tie-link-0")
        XCTAssertTrue(link.waitForExistence(timeout: 5))
        scrollIntoView(link)
        link.click()
        waitFor("the pair is untied", timeout: 20) {
            self.statusNotes.contains { $0 == "Plate ties: every link on this plate is turned off." }
        }
        XCTAssertTrue(element("plate-tie-link-0").exists, "the link stays listed, off")
        let restore = element("plate-ties-restore-links")
        XCTAssertTrue(restore.waitForExistence(timeout: 5), "Restore links")
        scrollIntoView(restore)
        restore.click()
        waitFor("tied again", timeout: 20) { self.statusNotes.contains { $0.hasPrefix("Plate ties: 3 of 3 ties on 1 pairs") } }
    }

    func testArrangeAndMoveAreLockedWhileTiesAreOn() throws {
        try tiedCoins()
        let arrange = element("toolbar-arrange")
        XCTAssertTrue(arrange.waitForExistence(timeout: 10), "Arrange with two models")
        waitFor("Arrange is greyed", timeout: 5) { !arrange.isEnabled }
        waitFor("Move is greyed", timeout: 5) { !self.element("toolbar-move").isEnabled }
        menuBarItem("Model").click()
        XCTAssertFalse(menuBarItem("Model").menuItems["Arrange"].isEnabled, "Model > Arrange is greyed")
        XCTAssertFalse(menuBarItem("Model").menuItems["Move"].isEnabled, "Model > Move is greyed")
        app.typeKey(.escape, modifierFlags: [])
        // off: unlocked
        let toggle = element("plate-ties-toggle")
        scrollIntoView(toggle)
        toggle.click()
        waitFor("Arrange is back", timeout: 5) { arrange.isEnabled && self.element("toolbar-move").isEnabled }
    }

    func testExportWithTiesWritesOneObject() throws {
        try tiedCoins()
        element("export-menu").click()
        let item = app.menuItems.matching(identifier: "3MF").allElementsBoundByIndex.first { $0.isHittable }
        try XCTUnwrap(item, "the export menu's 3MF item").click()
        XCTAssertTrue(panelExists("save-panel", timeout: 10), "the Save panel")
        saveInPanel(into: dir)
        let file = try XCTUnwrap(waitForFile(in: dir, timeout: 60, minSize: 100) { $0.pathExtension == "3mf" }, "the 3MF is written")
        // a stored zip: the model's XML is plain text inside it
        let text = String(decoding: try Data(contentsOf: file), as: UTF8.self)
        XCTAssertEqual(text.components(separatedBy: "<item ").count - 1, 1, "one merged object on the plate")
        XCTAssertTrue(text.contains("support_fins_plate_ties.json"), "the ties' settings go with it")
    }

    // MARK: the printer's bed and layer (decision A of #50)

    /// Picking the printer (the Saturn 4 Ultra 16K, already shown: picking it again counts) sets the toolbar's build
    /// volume to its 211.68 x 118.37 x 220 mm and the Layer height to its 0.05 mm; a hand edit afterwards holds.
    func testPickingThePrinterSetsTheBedAndTheLayerHeight() throws {
        try openFixturePart()
        let row = element("printer-row")
        XCTAssertTrue(row.waitForExistence(timeout: 10), "the printer row")
        // both are remembered: `tearDownWithError` leaves the next test the app's own bed and layer
        pickedPrinter = true
        row.click()   // unfolds the Printer section
        let printer = element("printer-profile")
        XCTAssertTrue(printer.waitForExistence(timeout: 5), "the Printer picker")
        printer.click()
        printer.menuItems["Elegoo Saturn 4 Ultra 16K"].click()
        waitFor("the build volume is the printer's", timeout: 5) { self.text(self.element("volume")).contains("211 × 118 × 220 mm") }
        waitFor("the layer height is the printer's", timeout: 5) { self.text(self.element("layer-height")) == "0.05" }
        XCTAssertTrue(text(row).contains("0.05 layers"), text(row))
        // by hand afterwards: the field takes it, the printer stays picked
        setLayerHeight("0.1")
        waitFor("the hand-set layer holds", timeout: 5) { self.text(row).contains("0.1 layers") }
        XCTAssertTrue(text(printer).contains("Saturn 4 Ultra 16K"), text(printer))
    }

    /// The printer test ran: its bed and layer are put back before the app quits.
    private var pickedPrinter = false

    override func tearDownWithError() throws {
        if pickedPrinter, app.state == .runningForeground {
            let volume = element("volume")
            volume.click()
            volume.menuItems["250 × 220 × 270 mm"].click()
            setLayerHeight("0.2")
        }
        try super.tearDownWithError()
    }

    private func setLayerHeight(_ v: String) {
        let field = element("layer-height")
        guard field.waitForExistence(timeout: 5) else { return }
        scrollIntoView(field)
        field.doubleClick()
        field.typeKey("a", modifierFlags: .command)
        field.typeText("\(v)\r")
    }
}
