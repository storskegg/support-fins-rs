import Foundation
import SupportFinsCore
import Testing
import Viewer

@testable import Support_Fins

/// Scale (`web/ui/scale.js`) through the window's pieces: the percent field's rules, the part reloaded at the new
/// size with its pose kept and the undo history started over, every scale taken from the part as loaded.
@MainActor
@Suite("Scale")
struct ScaleModelTests {
    private func eventually(_ what: String, _ condition: () -> Bool) async throws {
        for _ in 0..<1000 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        Issue.record("timed out: \(what)")
    }

    private func window() async throws -> (ViewerModel, ScaleFeature) {
        let host = ViewerModel()
        try #require(host.controller != nil, "no Metal device")
        let stl = URL(fileURLWithPath: try TestParts.write(TestParts.box(-10, 10, -5, 5, 0, 20), name: "scale-\(UUID().uuidString).stl"))
        await host.load(url: stl)
        return (host, try #require(host.features[ScaleFeature.self]))
    }

    private func size(_ host: ViewerModel) -> [Double] { host.session.parts[0].info.size }

    @Test func aNewPartIsAt100AndTheBadgeIsEmpty() async throws {
        let (host, f) = try await window()
        defer { withExtendedLifetime(host) {} }
        #expect(f.model.scale == 1 && f.model.hasPart && !f.model.isScaled)
        #expect(f.model.fieldText == "100" && f.model.badgeSuffix == "")
        #expect(host.toolbar.badges[.scale] != nil)
    }

    @Test func aTypedPercentReloadsThePartAtThatSizeAndTheBadgeSaysIt() async throws {
        let (host, f) = try await window()
        let before = size(host)
        f.model.commit(text: "150")
        try await eventually("the reload") { f.model.scale == 1.5 }
        for k in 0..<3 { #expect(abs(size(host)[k] - 1.5 * before[k]) < 1e-3) }
        #expect(f.model.badgeSuffix == " 150%" && host.toolbar.badges[.scale]?.detail() == " 150%")
        #expect(host.toolbar.badges[.scale]?.isLit() == true)
        #expect(host.controller?.size.x ?? 0 > 29.9)
        // the base is the part as loaded, so a second scale is not compounded
        f.model.commit(text: "50")
        try await eventually("the second reload") { f.model.scale == 0.5 }
        for k in 0..<3 { #expect(abs(size(host)[k] - 0.5 * before[k]) < 1e-3) }
        f.model.reset()
        try await eventually("reset") { f.model.scale == 1 }
        for k in 0..<3 { #expect(abs(size(host)[k] - before[k]) < 1e-3) }
        #expect(f.model.badgeSuffix == "" && !f.model.isScaled)
    }

    @Test func theFieldClampsTo10Through1000AndJunkKeepsTheScaleThatHolds() async throws {
        let (host, f) = try await window()
        defer { withExtendedLifetime(host) {} }
        f.model.commit(text: "5")
        try await eventually("clamped low") { f.model.scale == 0.1 }
        #expect(f.model.fieldText == "10")
        f.model.commit(text: "99999")
        try await eventually("clamped high") { f.model.scale == 10 }
        #expect(f.model.fieldText == "1000")
        #expect(f.model.commit(text: "abc") == "1000" && f.model.scale == 10)
        #expect(f.model.commit(text: "") == "1000" && f.model.scale == 10)
        #expect(f.model.commit(text: "-4") == "1000" && f.model.scale == 10)
        #expect(f.model.commit(text: "33.333") == "1000")        // shows what holds until the reload lands
        try await eventually("33.33") { f.model.scale == 0.3333 }
        #expect(f.model.fieldText == "33.33")
    }

    @Test func thePoseIsKeptAndHistoryStartsOver() async throws {
        let (host, f) = try await window()
        host.session.rotate90(about: .x)
        let pose = host.session.pose
        #expect(host.session.canUndo)
        f.model.commit(text: "200")
        try await eventually("the reload") { f.model.scale == 2 }
        #expect(host.session.pose == pose, "the pose is kept")
        #expect(!host.session.canUndo, "undo starts over, as for a new file")
        #expect(host.controller?.rotation == pose.float3x3)
    }

    @Test func aNewFileResetsTheScaleTo100() async throws {
        let (host, f) = try await window()
        f.model.commit(text: "200")
        try await eventually("the reload") { f.model.scale == 2 }
        let stl = URL(fileURLWithPath: try TestParts.write(TestParts.box(-5, 5, -5, 5, 0, 5), name: "scale2-\(UUID().uuidString).stl"))
        await host.load(url: stl)
        #expect(f.model.scale == 1 && f.model.badgeSuffix == "")
    }

    @Test func theModelMenuHasScaleThatOpensThePopover() async throws {
        let (host, f) = try await window()
        let sections = ScaleFeature.menus(f)
        guard case .button(let item) = sections[0].entries[1] else { Issue.record("no item"); return }
        #expect(item.title == "Scale…" && item.enabled)
        item.action()
        #expect(host.toolbar.open == .scale)
    }
}
