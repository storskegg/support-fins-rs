import XCTest

/// Draw through the real window (docs/UI-MAP.md rows `fin-mode`, `draw-back`, `draw-clear`, `augment-toggle`): open a
/// part, Add fins, switch Placement to Draw, look at the part from below (View > Bottom) so its red underside faces the
/// camera, click two points across it, read "1 drawn wall" in the Fins row, take a point back, clear.
///
/// Elements are found by the accessibility identifiers the app sets: `fins-toggle`, `fin-mode-draw`, `draw-hint`,
/// `draw-back`, `draw-clear`, `stat-fins`, `viewport`. The clicks are computed from the viewer's frame: the part is
/// framed and centred in it, so seen from below (0.38, 0.32) and (0.62, 0.32) lie on the slab's underside, clear of the
/// stem, for any likely window shape (HandPickingTests `theDrawUITestsClicksLandOnTheSlabsUnderside` checks it headless).
final class DrawUITests: PartWindowUITestCase {
    func testDrawAWallAndTakeItBack() throws {
        try openFixturePart()
        element("fins-toggle").click()

        // Placement > Draw: the Draw controls and their hint appear
        let draw = element("fin-mode-draw")
        scrollIntoView(draw)
        draw.click()
        let hint = element("draw-hint")
        XCTAssertTrue(hint.waitForExistence(timeout: 10), "the Draw hint")
        XCTAssertTrue(text(hint).hasPrefix("Click two points across an overhang"))
        let fins = element("stat-fins")
        waitFor("nothing drawn yet", timeout: 30) { self.text(fins) == "none yet" }

        // look at the part from below, then click two points across the slab's red underside
        // View > Bottom, looked up in the View menu only: `app.menuBars.menuItems["Bottom"]` also finds Window > Move &
        // Resize > Bottom (macOS's own), so it matched more than one item
        let view = menuBarItem("View")
        view.click()
        view.menuItems["Bottom"].click()
        Thread.sleep(forTimeInterval: 1.5)         // the camera glides 450 ms
        let viewport = element("viewport")
        XCTAssertTrue(viewport.waitForExistence(timeout: 10), "the viewer")
        let a = viewport.coordinate(withNormalizedOffset: CGVector(dx: 0.38, dy: 0.32))
        let b = viewport.coordinate(withNormalizedOffset: CGVector(dx: 0.62, dy: 0.32))
        a.click()
        let back = element("draw-back")
        XCTAssertTrue(back.waitForExistence(timeout: 10), "Back one point is offered while a wall is half drawn")
        scrollIntoView(back)        // the Draw controls sit below Placement, under the long hint
        back.click()
        waitFor("Back one point is gone with the point", timeout: 10) { !back.exists }
        a.click()
        XCTAssertTrue(back.waitForExistence(timeout: 10))
        b.click()
        waitFor("the Fins row counts the drawn wall", timeout: 60) { self.text(fins).hasPrefix("1 drawn wall") }
        XCTAssertFalse(back.exists, "the wall is placed: no point to take back")

        // Clear all
        let clear = element("draw-clear")
        scrollIntoView(clear)
        clear.click()
        waitFor("cleared", timeout: 30) { self.text(fins) == "none yet" }
    }
}
