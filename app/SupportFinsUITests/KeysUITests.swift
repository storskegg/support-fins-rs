import XCTest

/// Keys of the whole window (docs/UI-MAP.md section 7): Backspace takes back the wall's last point wherever the focus
/// is outside a text field (the web's window `keydown`),.
/// Identifiers: `fins-toggle`, `fin-mode-draw`, `draw-back`, `viewport`.
final class KeysUITests: PartWindowUITestCase {
    func testBackspaceTakesBackAPointFromTheInspector() throws {
        try openFixturePart()
        element("fins-toggle").click()
        let draw = element("fin-mode-draw")
        scrollIntoView(draw)
        draw.click()
        let view = menuBarItem("View")
        view.click()
        view.menuItems["Bottom"].click()
        Thread.sleep(forTimeInterval: 1.5)
        let viewport = element("viewport")
        XCTAssertTrue(viewport.waitForExistence(timeout: 10))
        viewport.coordinate(withNormalizedOffset: CGVector(dx: 0.38, dy: 0.32)).click()
        let back = element("draw-back")
        XCTAssertTrue(back.waitForExistence(timeout: 10), "a point is down")
        // the focus leaves the 3D view for the inspector; Backspace still reaches the wall
        draw.click()
        app.typeKey(.delete, modifierFlags: [])
        waitFor("the point is taken back", timeout: 10) { !back.exists }
    }
}
