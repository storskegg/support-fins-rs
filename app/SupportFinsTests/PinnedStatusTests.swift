import AppKit
import Foundation
import SwiftUI
import Testing

@testable import Support_Fins

/// The status card pinned above the inspector's tabs (docs/UI-MAP.md 4.3): the Fins line and the status notes stay on
/// show whichever tab is open; the Part tab keeps the part's stats. A long run of notes scrolls inside the card.
@MainActor
@Suite("Pinned status card")
struct PinnedStatusTests {
    private func host<V: View>(_ view: V, width: CGFloat = 300, height: CGFloat = 700) async throws -> NSHostingView<V> {
        _ = NSApp.perform(NSSelectorFromString("accessibilitySetValue:forAttribute:"), with: true as NSNumber,
                          with: "AXEnhancedUserInterface" as NSString)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: width, height: height), styleMask: [.titled],
                              backing: .buffered, defer: false)
        let host = NSHostingView(rootView: view)
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        try await Task.sleep(for: .milliseconds(200))
        return host
    }

    /// The accessibility identifiers under `root`.
    private func ids(under root: Any) -> [String] {
        func get(_ o: NSObject, _ sel: String) -> Any? {
            let s = NSSelectorFromString(sel)
            return o.responds(to: s) ? o.perform(s)?.takeUnretainedValue() : nil
        }
        var out: [String] = []
        func walk(_ e: Any, _ depth: Int) {
            guard depth < 40, let o = e as? NSObject else { return }
            if let id = get(o, "accessibilityIdentifier") as? String, !id.isEmpty { out.append(id) }
            for c in (get(o, "accessibilityChildren") as? [Any]) ?? [] { walk(c, depth + 1) }
        }
        walk(root, 0)
        return out
    }

    @Test func theFinsLineIsThePinnedCardsAndThePartTabKeepsTheStats() async throws {
        let model = ViewerModel()
        let controller = try #require(model.controller, "no Metal device")
        let stl = URL(fileURLWithPath: try TestParts.write(TestParts.box(-5, 5, -5, 5, 0, 10), name: "pinned-\(UUID().uuidString).stl"))
        defer { Scratch.remove(stl) }
        await model.load(url: stl)
        try #require(model.hasPart)
        let inputs = ReadoutInputs(controller: controller, session: model.session, build: model.wiring?.build, state: ReadoutState())
        let pinned = ids(under: try await host(PinnedStatus(inputs: inputs)))
        #expect(pinned.contains("stat-fins"), "the Fins line is pinned: \(pinned)")
        #expect(pinned.contains("stat-add-fins"), "with Add fins while they are off: \(pinned)")
        let tab = ids(under: try await host(Form { ReadoutSections(inputs: inputs) }.formStyle(.grouped)))
        #expect(tab.contains("stat-name"), "the Part tab keeps the part's stats: \(tab)")
        #expect(!tab.contains("stat-fins") && !tab.contains("stat-add-fins"), "and not the Fins line: \(tab)")
    }

    @Test func manyNotesScrollInsideTheCardAndAFewShowWhole() async throws {
        let long = (0..<12).map { StatusLine("n\($0)", text: String(repeating: "A long warning about slivers and midair starts. ", count: 3)) }
        let many = try await host(StatusCard(part: PartReadout.make(PartReadoutInput()), fins: nil, extra: long).frame(width: 280).fixedSize(horizontal: false, vertical: true))
        let manyHeight = many.fittingSize.height
        #expect(manyHeight <= StatusCard.notesMaxHeight + 80, "the notes scroll within the card: \(manyHeight) pt")
        #expect(ids(under: many).filter { $0 == "status-note" }.count >= 12, "every note is there, scrolled to")

        let few = try await host(StatusCard(part: PartReadout.make(PartReadoutInput()), fins: nil, extra: [StatusLine("one", text: "One note.")])
            .frame(width: 280).fixedSize(horizontal: false, vertical: true))
        let fewHeight = few.fittingSize.height
        #expect(fewHeight < manyHeight, "a short card is no taller than its notes: \(fewHeight) pt against \(manyHeight) pt")
        #expect(fewHeight > 30, "the Fins line and the note show: \(fewHeight) pt")
    }
}
