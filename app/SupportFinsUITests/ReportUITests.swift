import AppKit
import XCTest

/// "Something didn't get supported?" and Copy settings (`web/ui/report.js`) through the real window: the row appears
/// once a build is on screen, Copy settings puts the build's JSON on the clipboard and says "Copied".
/// (The link itself opens the browser: its bytes are checked by `crates/sf-ffi/tests/report.rs`.)
final class ReportUITests: PartWindowUITestCase {
    func testCopySettingsPutsTheBuildsJSONOnTheClipboard() throws {
        try openFixturePart()
        XCTAssertFalse(element("report-copy").exists, "no build yet: no row")

        element(Self.windowMark).click()          // Add fins
        let copy = element("report-copy")
        XCTAssertTrue(copy.waitForExistence(timeout: 60), "the row shows once the build is on screen")
        XCTAssertTrue(element("report-link").exists)

        NSPasteboard.general.clearContents()
        scrollIntoView(copy)
        copy.click()
        waitFor("the button says Copied", timeout: 5) { copy.label == "Copied" }
        let json = try XCTUnwrap(NSPasteboard.general.string(forType: .string))
        XCTAssertTrue(json.hasPrefix("{\n  \"v\": 1,"), "JSON.stringify(rep, null, 2)")
        XCTAssertTrue(json.contains("\"mode\": \"auto\""))
        waitFor("it reads Copy settings again", timeout: 5) { copy.label == "Copy settings" }
    }
}
