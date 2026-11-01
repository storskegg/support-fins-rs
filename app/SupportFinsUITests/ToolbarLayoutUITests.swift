import XCTest

/// The window toolbar's top right through the real window (user suggestion S5): the build volume, the Overhang slider,
/// Add fins, Export and the inspector toggle do not overlap with the inspector open or closed, and the inspector toggle
/// stays in the toolbar either way. `WindowToolbarLayoutTests` checks the same in-process at several widths; this is
/// the real document window at its default size. Run by the orchestrator, like the other UI tests.
///
/// Elements are found by their identifiers: `volume`, `thr`, `fins-toggle`, `export-menu`, `inspector-toggle`.
final class ToolbarLayoutUITests: PartWindowUITestCase {
    private static let ids = ["volume", "thr", "fins-toggle", "export-menu", "inspector-toggle"]

    private func assertNoOverlaps(_ when: String, file: StaticString = #filePath, line: UInt = #line) {
        let frames = Self.ids.map { (id: $0, frame: element($0).frame) }
        for f in frames { XCTAssertFalse(f.frame.isEmpty, "\(f.id) shows (\(when))", file: file, line: line) }
        for (i, a) in frames.enumerated() {
            for b in frames[(i + 1)...] {
                XCTAssertFalse(a.frame.intersects(b.frame), "\(a.id) \(a.frame) overlaps \(b.id) \(b.frame) (\(when))", file: file, line: line)
            }
        }
        // left to right in the window's order (English)
        let xs = frames.map(\.frame.midX)
        XCTAssertEqual(xs, xs.sorted(), "the toolbar's order (\(when))", file: file, line: line)
    }

    func testTheToolbarDoesNotOverlapWithTheInspectorOpenOrClosed() throws {
        try openFixturePart()
        let toggle = element("inspector-toggle")
        XCTAssertTrue(toggle.waitForExistence(timeout: 10), "the inspector toggle")
        XCTAssertTrue(element("thr").waitForExistence(timeout: 10))
        assertNoOverlaps("inspector open")

        toggle.click()
        waitFor("the inspector toggle is still in the toolbar, inspector closed", timeout: 5) { toggle.exists && toggle.isHittable }
        waitForFrameToSettle(element("thr"))
        assertNoOverlaps("inspector closed")

        toggle.click()
        waitForFrameToSettle(element("thr"))
        assertNoOverlaps("inspector open again")
    }
}
