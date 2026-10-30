import Foundation
import simd
import SupportFinsCore
import Testing
import UniformTypeIdentifiers

@testable import Support_Fins

/// Sessions (`web/ui/session.js`): the 3MFs the page saved (`parity/corpus/session/`, recorded by
/// `parity/oracle/session.js` from the page's own code) open in the window with the state the page restored, and
/// the 3MFs the window writes open to the same state again. The engine's side is `crates/sf-ffi/tests/session.rs`.
@MainActor
@Suite("Sessions")
struct SessionTests {
    static let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    static var corpus: URL { root.appending(path: "parity/corpus/session") }

    static func cases() throws -> [[String: Any]] {
        let data = try Data(contentsOf: corpus.appending(path: "cases.json"))
        let json = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        return try #require(json["cases"] as? [[String: Any]])
    }

    static func named(_ name: String) throws -> [String: Any] {
        try #require(try cases().first { $0["name"] as? String == name }, "no case \(name)")
    }

    func open(_ file: URL) async throws -> ViewerModel {
        let model = ViewerModel()
        try #require(model.controller != nil, "no Metal device")
        await model.load(url: file)
        #expect(model.status == nil, "\(model.status ?? "")")
        return model
    }

    func open(case c: [String: Any]) async throws -> ViewerModel {
        try await open(Self.corpus.appending(path: try #require(c["file"] as? String)))
    }

    /// The page's expectation for a restored case: `read.state`.
    func state(_ c: [String: Any]) throws -> [String: Any] {
        let read = try #require(c["read"] as? [String: Any])
        return try #require(read["state"] as? [String: Any])
    }

    func note(_ model: ViewerModel) -> String { model.session.parts.first?.info.importNote ?? "" }

    // MARK: the settings

    @Test func theSettingsMapToTheWebsFieldsAndBack() {
        var s = FinSettings()
        let form = s.sessionForm
        #expect(form.map(\.id) == FinSettings.sessionIDs && form.count == 29)
        let text = Dictionary(uniqueKeysWithValues: form.map { ($0.id, $0.value) })
        // the page's defaults, as its fields hold them (`readForm` on a fresh page)
        #expect(text["thr"] == .text("45") && text["tines"] == .flag(true) && text["tine-density"] == .text("0"))
        #expect(text["pad-h"] == .text("0.2") && text["pad-gap"] == .text("0.12") && text["wall-lines"] == .text("2"))
        #expect(text["bed-pad"] == .text("auto") && text["cutout"] == .text("none") && text["iface"] == .text("off"))
        #expect(text["tall-ribs-height"] == .text("10") && text["long-walls"] == .flag(true) && text["coverage"] == .text("50"))
        s.gap = 0.25
        s.layerHeight = 0.16
        s.bedPad = .custom
        s.padSpread = 6.5
        s.tineDirection = .vertical
        s.interfaceMaterial = .flat
        s.wallLines = 4
        s.sway = true
        s.curvedWalls = true
        var back = FinSettings()
        back.apply(session: s.sessionForm.map {
            switch $0.value {
            case .flag(let b): SessionField(id: $0.id, flag: b, text: nil)
            case .text(let t): SessionField(id: $0.id, flag: nil, text: t)
            }
        })
        #expect(back == s, "every setting a session carries comes back")
        // the viewer's own and the printer's nozzle are not in a session
        s.nozzle = 0.6
        s.showIslands = true
        #expect(s.sessionForm.map(\.id) == FinSettings.sessionIDs)
        var kept = FinSettings()
        kept.nozzle = 0.8
        kept.apply(session: s.sessionForm.map { SessionField(id: $0.id, flag: nil, text: nil) })
        #expect(kept.nozzle == 0.8, "a field without a value changes nothing")
    }

    // MARK: web -> native

    @Test func aSessionTheWebSavedRestoresInTheWindow() async throws {
        let c = try Self.named("draw mode, walls and braces")
        let model = try await open(case: c)
        let want = try state(c)
        let s = model.session
        #expect(s.settings.mode == .draw && s.finsOn)
        #expect(s.pose == .identity && !s.addByHand)
        let walls = try #require(want["walls"] as? [[String: Any]])
        #expect(s.drawnWalls.count == walls.count && walls.count == 4)
        #expect(s.drawnWalls.map(\.kind) == [.wall, .wall, .sway, .sway])
        #expect(s.drawnWalls[1].m != nil && s.drawnWalls[0].m == nil)
        #expect(s.drawnWalls.map(\.capped) == [false, false, true, false])
        #expect(s.drawnWalls[2].face == 2 && s.drawnWalls[3].face == 5, "a brace stands on the face nearest its point")
        #expect(!s.canUndo, "a restore is not an undo step")
        #expect(note(model) == "3MF: restored 4 hand-placed supports and the settings.")
        #expect(model.session.parts.first?.part.hasSession() == false)
    }

    @Test func theSettingsTheWebSavedComeBack() async throws {
        let c = try Self.named("interface and printer settings")
        let model = try await open(case: c)
        let s = model.session.settings
        #expect(s.threshold == 50 && !s.tines && s.tineDensity == 35 && s.tineDirection == .vertical)
        #expect(s.pinDepth == 0.2 && s.layerHeight == 0.16 && s.gap == 0.25 && s.bedPad == .custom)
        #expect(s.padThickness == 0.3 && s.padGap == 0.15 && s.padGrip == 0.05 && s.padSpread == 6.5)
        #expect(s.sway && s.swayFrom == 20 && s.swaySpacing == 8 && s.swayDepth == 20 && s.swayCrossHeight == 60 && s.fullBraceDepth)
        #expect(s.roundedFeet && s.wallLines == 4 && s.cutout == .arch && s.interfaceMaterial == .flat && s.coverage == 75)
        #expect(s.plateOnly && !s.longWalls && s.separateWalls && !s.tallRibs && s.tallRibsHeight == 12.5 && s.curvedWalls)
        #expect(s.mode == .auto && model.session.finsOn)
        #expect(note(model) == "3MF: imported “lbracket” of 2 objects; restored the settings.")
    }

    @Test func removedFinsAndFinsHiddenComeBack() async throws {
        let c = try Self.named("auto fins, two removed")
        let model = try await open(case: c)
        let want = try #require(try state(c)["removedSigs"] as? [String])
        #expect(want.count == 2 && model.session.removedFins == Set(want.map { FinKey(raw: $0) }))
        let hidden = try await open(case: try Self.named("one model, fins hidden"))
        #expect(!hidden.session.finsOn)
    }

    @Test func aFieldTheFormCannotTakeKeepsTheAppsValue() async throws {
        let c = try Self.named("form with values a field cannot take")
        let before = ViewerModel().session.settings
        let model = try await open(case: c)
        let s = model.session.settings
        #expect(s.tineDensity == 35, "a slider snaps to its steps")
        #expect(s.cutout == .arch && s.padThickness == 0.25 && s.layerHeight == 0.12)
        #expect(s.threshold == before.threshold && s.tines == before.tines && s.tineDirection == before.tineDirection)
        #expect(s.gap == before.gap && s.wallLines == 2 && s.nozzle == before.nozzle && !model.session.finsOn)
        // a session saved before Long walls had no such field: it came back off
        #expect(!s.longWalls && !s.roundedFeet && s.tallRibs)
    }

    @Test func aPartThatChangedOrASessionThatIsDamagedLoadsAsAPlainPart() async throws {
        let base = ViewerModel().session.settings
        for name in ["part changed since it was exported", "unknown fin mode", "a wall with one point"] {
            let c = try Self.named(name)
            let model = try await open(case: c)
            #expect(model.session.drawnWalls.isEmpty && model.session.settings == base && model.session.removedFins.isEmpty, "\(name)")
            let want = (c["read"] as? [String: Any])?["note"] as? String
            #expect(note(model) == want && want?.contains("saved supports not loaded") == true, "\(name): \(note(model))")
        }
    }

    @Test func everyFileOfTheCorpusOpensWithTheNoteThePageShowed() async throws {
        var opened = 0
        for c in try Self.cases() {
            let read = try #require(c["read"] as? [String: Any])
            guard let outcome = read["session"] as? String, outcome != "none" else { continue }
            let model = try await open(case: c)
            #expect(note(model) == (read["note"] as? String), "\(c["name"] ?? "")")
            if outcome == "restored", let st = read["state"] as? [String: Any] {
                #expect(model.session.drawnWalls.count == (st["walls"] as? [Any])?.count, "\(c["name"] ?? "")")
                #expect(model.session.removedFins == Set((st["removedSigs"] as? [String] ?? []).map { FinKey(raw: $0) }))
                #expect(model.session.settings.mode.rawValue == st["finMode"] as? String)
                #expect(model.session.finsOn == (st["finsVisible"] as? Bool))
            }
            opened += 1
        }
        #expect(opened >= 25)
    }

    @Test func aFileWithoutASessionIsAPlainPart() async throws {
        let c = try Self.named("two models on a plate")
        let model = ViewerModel()
        await model.load(url: Self.corpus.appending(path: try #require(c["file"] as? String)))
        #expect(model.picker != nil, "two objects ask which to load, as any 3MF does")
    }

    // MARK: a part beside a plate's models

    @Test func aSessionAddedBesideAPlateKeepsThePlatesSettings() async throws {
        let helper = PlateBoardTests()
        let (model, board) = try await helper.window(name: "first")
        let plate = model.session.settings
        let c = try Self.named("draw mode, walls and braces")
        await model.load(url: Self.corpus.appending(path: try #require(c["file"] as? String)), adding: true)
        #expect(board.modelCount == 2)
        let s = model.session
        #expect(s.settings == plate && s.settings.mode == .auto, "the plate keeps its mode and settings")
        #expect(s.drawnWalls.count == 4, "the walls are the model's")
        #expect(note(model) == "3MF: restored 4 hand-placed supports; the plate keeps its settings.")
    }

    // MARK: the rescale forgets the note

    @Test func rescalingForgetsWhatTheSessionBroughtBack() async throws {
        let model = try await open(case: try Self.named("draw mode, walls and braces"))
        let core = try #require(model.session.parts.first)
        #expect(note(model).contains("restored 4"))
        let device = try #require(model.controller?.renderer.device)
        let scaled = try #require(CorePart(part: try core.part.scaled(factor: 1.5), device: device))
        model.reshow(scaled)
        #expect(!note(model).contains("restored") && model.session.drawnWalls.isEmpty, "\(note(model))")
    }

    // MARK: native -> native, and the plate's export

    func exported(_ model: ViewerModel, to name: String) throws -> URL {
        model.session.finsOn = false      // the part alone: the session is what is under test
        let url = try Scratch.file(name)
        let wrote = try #require(model.wiring?.exporter?.write(.threeMF, to: url), "\(model.wiring?.exporter?.error ?? "")")
        return wrote
    }

    @Test func aSessionTheWindowWritesOpensToTheSameState() async throws {
        let model = try await open(case: try Self.named("draw mode, walls and braces"))
        model.session.settings.gap = 0.3
        model.session.settings.layerHeight = 0.16
        model.session.settings.bedPad = .light
        model.session.removedFins = [FinKey(raw: "prop:1.5,2,3:4:0.5"), FinKey(raw: "wedge:0,0,0:1:0")]
        let url = try exported(model, to: "round-trip.3mf")
        let back = try await open(url)
        let a = model.session, b = back.session
        #expect(b.settings.gap == 0.3 && b.settings.layerHeight == 0.16 && b.settings.bedPad == .light && b.settings.mode == .draw)
        #expect(!b.finsOn && b.removedFins == a.removedFins)
        #expect(b.drawnWalls.count == 4 && b.drawnWalls.map(\.kind) == a.drawnWalls.map(\.kind))
        #expect(b.drawnWalls.map(\.capped) == a.drawnWalls.map(\.capped) && b.drawnWalls.map(\.face) == a.drawnWalls.map(\.face))
        // the same walls: the exported part is the shown one at the 3MF's bed shift, so the points move together
        let shift = b.drawnWalls[0].a - a.drawnWalls[0].a
        for (x, y) in zip(a.drawnWalls, b.drawnWalls) {
            #expect(simd_length(y.a - x.a - shift) < 5e-5)
            if x.kind == .wall { #expect(simd_length(y.b - x.b - shift) < 5e-5) }
        }
        #expect(note(back) == "3MF: restored 4 hand-placed supports and the settings.")
    }

    @Test func aPlateOfOneWritesItsSessionAndAPlateOfSeveralNone() async throws {
        let helper = PlateBoardTests()
        let (model, board) = try await helper.window(name: "first")
        model.session.drawnWalls = [.wall(from: [1, 2, 3], to: [4, 5, 6])]
        model.session.settings.mode = .draw
        let one = try #require(board.plateWrite(0))
        let text = try #require(one.models[0].session)
        #expect(text.contains("\"finMode\":\"draw\"") && text.contains("\"walls\":[{\"kind\":\"wall\""))
        try await helper.add(model, "second")
        let two = try #require(board.plateWrite(0))
        #expect(two.models.count == 2 && two.models.allSatisfy { $0.session == nil })
    }
}
