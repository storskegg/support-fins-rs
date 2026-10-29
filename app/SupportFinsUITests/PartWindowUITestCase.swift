import XCTest

/// The base of every UI test: the app launched with nothing restored, a part opened through the Open panel into
/// exactly one window, element queries scoped to that window, scrolling by identifier, and the window closed again at
/// the end. Tests subclass it and call `openFixturePart()` first.
///
/// What it guards against (FinsLoopUITests failed most runs under load):
/// - **Moving targets.** Scrolling the inspector by scrolling over one of its rows (`stat-fins`) asks XCUITest for the
///   row's hit point while the previous scroll is still settling: the row's frame is stale and the hit test lands on
///   another row ("Unable to find hit point for StaticText ... 'stat-fins'"). `scrollIntoView` scrolls the scroll view
///   itself (its centre does not move) and waits for the target's frame to settle between steps.
/// - **Racing the file system.** A Save panel's Return starts the export; the file lands later. `waitForFile` waits.
/// - **Leftover windows.** The app starts with state restoration off (`-ApplePersistenceIgnoreState YES`, and
///   `NSQuitAlwaysKeepsWindows` off), a test fails at once if a part window is already open before it opens its
///   own, and `tearDown` closes the part windows before quitting so no run leaves any to restore.
/// - **Ambiguous queries.** `element(_:)` looks only inside the part's window; `menuBarItem(_:)` asserts the menu
///   bar has exactly one menu of that title.
///
/// XCUITest needs the Mac's Accessibility permission for the test runner ("Timed out while enabling automation mode"
/// otherwise); only the orchestrator runs these (`just app-uitest`). Agents compile them (`just app-uibuild`).
class PartWindowUITestCase: XCTestCase {
    var app: XCUIApplication!
    /// A fresh temp directory for the test's files (the fixture part, exports).
    var dir: URL!
    /// The fixture part, once `openFixturePart()` has written and opened it.
    private(set) var part: URL?

    /// Every part window has the Add fins toggle in its toolbar.
    static let windowMark = "fins-toggle"

    override func setUpWithError() throws {
        continueAfterFailure = false
        dir = FileManager.default.temporaryDirectory.appending(path: "sf-uitest-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        app = XCUIApplication()
        app.launchArguments = ["-ApplePersistenceIgnoreState", "YES", "-NSQuitAlwaysKeepsWindows", "NO"]
    }

    override func tearDownWithError() throws {
        if app.state != .notRunning {
            closePartWindows()
            app.terminate()
        }
        try? FileManager.default.removeItem(at: dir)
    }

    // MARK: the part's window

    /// The windows showing a part.
    var partWindows: XCUIElementQuery { app.windows.containing(.any, identifier: Self.windowMark) }

    /// The one window showing the part (`openFixturePart` checks there is exactly one).
    var partWindow: XCUIElement { partWindows.firstMatch }

    /// An element of the part's window by its accessibility identifier. Menus and panels are the app's, not the
    /// window's: query those from `app`.
    func element(_ id: String) -> XCUIElement { partWindow.descendants(matching: .any)[id].firstMatch }

    /// An element of a toolbar popover (a popover is a window of its own, which may not sit under the part's window
    /// in the accessibility tree): looked up in the app's popovers.
    func popoverElement(_ id: String) -> XCUIElement { app.popovers.descendants(matching: .any)[id].firstMatch }

    /// Launch with nothing open, open the fixture part through the Open panel (⇧⌘G to its path) and wait for its window:
    /// exactly one part window.
    func openFixturePart() throws { try openPart(writeFixtureSTL()) }

    /// Launch with nothing open, open `part` (any file the Open panel takes) and wait for its window: exactly one.
    func openPart(_ part: URL, timeout: TimeInterval = 20) throws {
        self.part = part
        app.launch()
        // the document app shows its Open panel at launch; nothing is restored
        let panel = panelExists("open-panel", timeout: 5)
        XCTAssertEqual(partWindows.count, 0, "no part window is open before the test opens one (state restoration is off)")
        if !panel { app.typeKey("o", modifierFlags: .command) }
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel")
        chooseInPanel(part.path)
        XCTAssertTrue(partWindow.waitForExistence(timeout: timeout), "the part's window opened")
        XCTAssertEqual(partWindows.count, 1, "one part window")
    }

    /// Close every part window (Esc for any popover or menu, then ⌘W to the key window; the app only views files, so
    /// nothing asks to save). No click: a click on the window would land in the 3D view.
    func closePartWindows() {
        for _ in 0..<5 where partWindows.count > 0 {
            let w = partWindows.firstMatch
            app.typeKey(.escape, modifierFlags: [])
            app.typeKey("w", modifierFlags: .command)
            _ = waitUntil(timeout: 5) { !w.exists }
        }
    }

    /// A static text's string: its value, or its label when the value is empty.
    func text(_ e: XCUIElement) -> String {
        guard e.exists else { return "" }
        if let v = e.value as? String, !v.isEmpty { return v }
        return e.label
    }

    // MARK: waiting

    func waitFor(_ what: String, timeout: TimeInterval, _ condition: @escaping () -> Bool) {
        XCTAssertTrue(waitUntil(timeout: timeout, condition), what)
    }

    /// True once `condition` holds, false when `timeout` runs out first.
    func waitUntil(timeout: TimeInterval, _ condition: @escaping () -> Bool) -> Bool {
        let e = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in condition() }, object: nil)
        return XCTWaiter().wait(for: [e], timeout: timeout) == .completed
    }

    /// Wait until an element's frame holds still (a scroll or a relayout has settled), at most `timeout`.
    func waitForFrameToSettle(_ e: XCUIElement, timeout: TimeInterval = 2) {
        var last = e.frame
        let end = Date().addingTimeInterval(timeout)
        while Date() < end {
            Thread.sleep(forTimeInterval: 0.15)
            let now = e.frame
            if now == last { return }
            last = now
        }
    }

    /// Wait for a file in `dir` matching `match`, written and not empty (more than `minSize` bytes).
    func waitForFile(in dir: URL, timeout: TimeInterval, minSize: Int = 0, _ match: @escaping (URL) -> Bool) -> URL? {
        var found: URL?
        _ = waitUntil(timeout: timeout) {
            let files = (try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: [.fileSizeKey])) ?? []
            found = files.first { url in
                match(url) && ((try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0) > minSize
            }
            return found != nil
        }
        return found
    }

    // MARK: scrolling

    /// Scroll `target` into view in the scroll view that holds it, by scrolling that scroll view (whose centre stays
    /// put), never by scrolling over a row that moves with the content; between steps, wait for the content to settle.
    /// In view means hittable, wholly below the scroll view's top and at least `bottomMargin` above its bottom: XCUITest
    /// calls an element hittable while something drawn over the view's bottom edge covers it (#184 saw a click on the
    /// Tine grip slider land on the debug Benchmark orbit bar).
    func scrollIntoView(_ target: XCUIElement, bottomMargin: CGFloat = 60, maxSteps: Int = 20,
                        file: StaticString = #filePath, line: UInt = #line) {
        // the innermost scroll view holding it (the smallest), not one that only contains that one
        let holders = partWindow.scrollViews.containing(.any, identifier: target.identifier).allElementsBoundByIndex
        guard let scroller = holders.min(by: { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height }) else {
            XCTFail("no scroll view holds \(target.identifier)", file: file, line: line)
            return
        }
        func inView() -> Bool {
            let f = target.frame, s = scroller.frame
            return target.isHittable && f.minY >= s.minY && f.maxY <= s.maxY - bottomMargin
        }
        for _ in 0..<maxSteps where !inView() {
            // content below the middle: scroll down (negative delta), above it: up
            let down = target.frame.midY > scroller.frame.midY
            scroller.scroll(byDeltaX: 0, deltaY: down ? -60 : 60)
            waitForFrameToSettle(target)
        }
        XCTAssertTrue(inView(), "\(target.identifier) scrolled into view", file: file, line: line)
    }

    // MARK: the menu bar

    /// The menu bar's menu of this title, asserting there is exactly one (a feature menu must never duplicate a menu
    /// macOS already has, as a second View did).
    func menuBarItem(_ title: String, file: StaticString = #filePath, line: UInt = #line) -> XCUIElement {
        let items = app.menuBars.menuBarItems.matching(NSPredicate(format: "title == %@", title))
        XCTAssertEqual(items.count, 1, "one \(title) menu in the menu bar", file: file, line: line)
        return items.firstMatch
    }

    // MARK: panels

    /// An Open or Save panel: a dialog or sheet, or (macOS 26) a window with the panel's identifier.
    func panelExists(_ id: String, timeout: TimeInterval) -> Bool {
        waitUntil(timeout: timeout) {
            self.app.dialogs.firstMatch.exists || self.app.sheets.firstMatch.exists || self.app.windows[id].exists
        }
    }

    /// In an Open panel: ⇧⌘G, the path, Return (go to it), Return (Open).
    func chooseInPanel(_ path: String) {
        app.typeKey("g", modifierFlags: [.command, .shift])
        app.typeText(path)
        app.typeKey(.return, modifierFlags: [])
        app.typeKey(.return, modifierFlags: [])
    }

    /// In a Save panel: ⇧⌘G to the directory (the suggested file name stays), then Return (Save).
    func saveInPanel(into dir: URL) {
        app.typeKey("g", modifierFlags: [.command, .shift])
        app.typeText(dir.path)
        app.typeKey(.return, modifierFlags: [])
        app.typeKey(.return, modifierFlags: [])
    }

    // MARK: the fixture

    /// A binary STL: a 10 mm stem under a 60 mm slab, so there is an overhang to put fins under.
    func writeFixtureSTL() throws -> URL { try writeSTL("bracket.stl", slab: 30) }

    /// A binary STL named `name` in `dir`: a 10 mm stem under a slab of half-width `slab` mm.
    func writeSTL(_ name: String, slab: Float) throws -> URL {
        func box(_ x0: Float, _ x1: Float, _ y0: Float, _ y1: Float, _ z0: Float, _ z1: Float) -> [[Float]] {
            let v: [[Float]] = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]]
            let faces = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]]
            return faces.map { f in f.flatMap { v[$0] } }
        }
        let tris = box(-5, 5, -5, 5, 0, 20) + box(-slab, slab, -slab, slab, 20, 24)
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
}
