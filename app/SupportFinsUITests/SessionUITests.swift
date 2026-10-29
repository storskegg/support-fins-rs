import XCTest

/// Sessions through the real window (`web/ui/session.js`): a 3MF the web exported with hand-drawn walls opens with
/// them, and the status card's import note says so (`renderImportNote`). The file is one the page's own code wrote
/// (`parity/corpus/session/`, `just oracle session`). The launch, the window and the helpers are
/// `PartWindowUITestCase`'s.
final class SessionUITests: PartWindowUITestCase {
    private var corpus: URL {
        URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appending(path: "parity/corpus/session")
    }

    func testASessionFileOpensWithItsWallsAndSaysSo() throws {
        let file = dir.appending(path: "draw-mode-walls-and-braces.3mf")
        try FileManager.default.copyItem(at: corpus.appending(path: "draw-mode-walls-and-braces.3mf"), to: file)
        try openPart(file)
        let note = element("import-note")
        XCTAssertTrue(note.waitForExistence(timeout: 20), "the import note")
        XCTAssertEqual(text(note), "3MF: restored 4 hand-placed supports and the settings.")
        // the session put Placement in Draw (the web's fin-mode), where "Clear walls" lists the walls it brought back
        XCTAssertTrue(element("draw-clear").waitForExistence(timeout: 10), "Draw's controls, with walls to clear")
    }
}
