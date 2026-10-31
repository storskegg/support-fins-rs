import Foundation
import SupportFinsCore
import Testing
import UniformTypeIdentifiers
import Viewer

@testable import Support_Fins

/// Plate ties in the window (issue #50, slice 2): the links a plate of upright coins gets, the links clicked off and
/// added, the lock on Arrange and Move, the rebuild when the plate changes, the merged export and the ties a 3MF
/// brings back. The coins are generated here (r 20, 3 mm thick, faces along x), as the engine's tests make them.
@MainActor
@Suite("Plate ties")
struct PlateTiesTests {
    let helper = PlateBoardTests()

    /// An upright coin as an STL: 96 sides, its faces at x = -1.5 and 1.5, standing on the bed.
    static func coin(_ name: String) throws -> URL {
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
        return URL(fileURLWithPath: try TestParts.write(tris, name: "\(name)-\(UUID().uuidString).stl"))
    }

    /// A window with `xs.count` coins on its plate, standing at x = `xs` (y 0).
    func coins(_ xs: [Double]) async throws -> (ViewerModel, PlateBoard, PlateTiesModel) {
        try await coins(xs, files: try xs.indices.map { try Self.coin("coin\($0)") })
    }

    /// A window with the coins of `files` on its plate (a file twice is two models of one name), at x = `xs`.
    func coins(_ xs: [Double], files: [URL]) async throws -> (ViewerModel, PlateBoard, PlateTiesModel) {
        let model = ViewerModel()
        try #require(model.controller != nil, "no Metal device")
        await model.load(url: files[0])
        for k in 1..<xs.count { await model.load(url: files[k], adding: true) }
        let board = try #require(model.features[PlatesFeature.self]?.board)
        let ties = try #require(model.features[PlateTiesFeature.self]?.model)
        try #require(board.plate.models.count == xs.count)
        for (k, m) in board.plate.models.enumerated() {
            if k == board.plate.live { board.placeLive(m, SIMD2(xs[k], 0)) } else { m.xy = SIMD2(xs[k], 0) }
        }
        board.sync()
        return (model, board, ties)
    }

    func on(_ ties: PlateTiesModel) async throws {
        ties.setOn(true)
        #expect(ties.state.on)
        #expect(await ties.settled(), "the ties land")
        #expect(ties.error == nil, "\(ties.error ?? "")")
    }

    func key(_ board: PlateBoard, _ k: Int) -> String { board.tieKeys(board.active)[k] ?? "" }

    // MARK: detect -> links

    @Test func turningTiesOnLinksTheNeighboursAndDrawsTheirTies() async throws {
        let (model, board, ties) = try await coins([-7, 0, 7])
        defer { withExtendedLifetime(model) {} }
        #expect(model.controller?.renderer.tieScene == nil, "nothing before they are on")
        try await on(ties)
        #expect(ties.links.count == 2 && ties.candidates.count == 2)
        // three ties a pair by default, all placed, no notes
        #expect(ties.requested == 6 && ties.placed == 6 && ties.notes.isEmpty, "\(ties.notes)")
        #expect(ties.triangles > 0)
        let scene = try #require(model.controller?.renderer.tieScene)
        #expect(scene.links.count == 2 && scene.links.allSatisfy { $0.enabled && !$0.manual })
        #expect(scene.ties.map { $0.vertices == 3 * ties.triangles } == true)
        // the links run across the gaps: from one coin's face to the next
        let xs = scene.links.flatMap { [$0.from.x, $0.to.x] }.sorted()
        #expect(xs.first.map { abs($0 + 5.5) < 0.5 } == true && xs.last.map { abs($0 - 5.5) < 0.5 } == true, "\(xs)")
        let summary = model.features.statusLines.map(\.text)
        #expect(summary.contains { $0.contains("6") && $0.contains("2") }, "\(summary)")
        // off: the links and ties go
        ties.setOn(false)
        #expect(model.controller?.renderer.tieScene == nil && ties.links.isEmpty)
        #expect(!board.plate.ties.on)
    }

    @Test func coinsTooFarApartGetNoLinks() async throws {
        let (model, _, ties) = try await coins([-30, 30])
        defer { withExtendedLifetime(model) {} }
        try await on(ties)
        #expect(ties.links.isEmpty)
        #expect(model.features.statusLines.contains { $0.id == "plate-ties-none" })
    }

    // MARK: overrides

    @Test func aLinkClickedOffStaysDrawnGreyAndComesBack() async throws {
        let (model, board, ties) = try await coins([-7, 0, 7])
        defer { withExtendedLifetime(model) {} }
        try await on(ties)
        let first = try #require(ties.links.first)
        ties.toggle(first)
        #expect(await ties.settled())
        #expect(ties.links.count == 1 && ties.candidates.count == 2 && ties.state.disabled.count == 1)
        #expect(board.plate.ties.disabled.count == 1, "kept with the plate")
        #expect(ties.requested == 3)
        let scene = try #require(model.controller?.renderer.tieScene)
        #expect(scene.links.filter(\.enabled).count == 1 && scene.links.filter { !$0.enabled }.count == 1)
        // a click on the grey one brings it back
        let grey = try #require(ties.drawn.first { l in !ties.links.contains { $0.a == l.a && $0.b == l.b } })
        ties.toggle(grey)
        #expect(await ties.settled())
        #expect(ties.links.count == 2 && ties.state.disabled.isEmpty)
    }

    @Test func aClickOnALinkInTheViewTurnsItOff() async throws {
        let (model, board, ties) = try await coins([-7, 0, 7])
        defer { withExtendedLifetime(model) {} }
        try await on(ties)
        let renderer = try #require(model.controller?.renderer)
        let size = 400
        renderer.camera = OrbitCamera(target: .zero, distance: 300, azimuth: -.pi / 2, elevation: OrbitCamera.maxElevation, aspect: 1)
        let line = try #require(renderer.tieScene?.links.first)
        let mid = (line.from + line.to) / 2 + SIMD3(renderer.plateScene.beds[board.active], 0)
        let n = renderer.camera.project(mid)
        let (x, y) = (Int((n.x + 1) / 2 * Float(size)), Int((1 - n.y) / 2 * Float(size)))
        board.tapped(x: x, y: y, width: size, height: size)
        #expect(await ties.settled())
        #expect(ties.links.count == 1 && ties.state.disabled.count == 1)
        // a click on bare bed does not touch the links
        board.tapped(x: 5, y: 5, width: size, height: size)
        #expect(ties.state.disabled.count == 1)
    }

    @Test func twoModelsPickedMakeAPairByHandAndItsNotesAreWorded() async throws {
        let (model, board, ties) = try await coins([-14, 14])
        defer { withExtendedLifetime(model) {} }
        try await on(ties)
        #expect(ties.links.isEmpty)
        ties.setPairing(true)
        #expect(model.features.statusLines.contains { $0.id == "plate-ties-pairing" })
        ties.picked(key(board, 0))
        ties.picked(key(board, 0))           // the same model twice is one pick
        #expect(ties.pairing == [key(board, 0)])
        ties.picked(key(board, 1))
        #expect(ties.pairing == nil && ties.state.added.count == 1)
        #expect(await ties.settled())
        let link = try #require(ties.links.first)
        #expect(link.origin == .manual)
        #expect(model.controller?.renderer.tieScene?.links.first?.manual == true)
        // 25 mm apart: longer than a tie may reach (1.5 x max gap), so each is left out, and the status says why
        #expect(ties.placed == 0 && ties.notes.count == 3, "\(ties.notes)")
        #expect(ties.notes.allSatisfy { $0.contains("coin") && $0.contains("left out") }, "\(ties.notes)")
        #expect(model.features.statusLines.contains { $0.id == "plate-ties-note-0" })
        // a click on the hand-made link takes it away
        ties.toggle(link)
        #expect(await ties.settled())
        #expect(ties.links.isEmpty && ties.state.added.isEmpty && ties.state.disabled.isEmpty)
        // Esc stops a pair half picked
        ties.setPairing(true)
        #expect(model.features.escape())
        #expect(ties.pairing == nil)
    }

    @Test func restoreLinksDropsEveryOverride() async throws {
        let (model, _, ties) = try await coins([-7, 0, 7])
        defer { withExtendedLifetime(model) {} }
        try await on(ties)
        ties.toggle(try #require(ties.links.first))
        #expect(ties.hasOverrides)
        ties.restoreLinks()
        #expect(await ties.settled())
        #expect(!ties.hasOverrides && ties.links.count == 2)
    }

    // MARK: settings

    @Test func settingsRoundTripThroughTheEngineAndResetToTheProfile() async throws {
        let (model, board, ties) = try await coins([-7, 0, 7])
        defer { withExtendedLifetime(model) {} }
        try await on(ties)
        var s = try #require(ties.settings)
        #expect(s.maxGap == 8 && s.tiesPerPair == 3 && s.contact == .edge)
        s.maxGap = 3                                    // the gaps are 4 mm: no neighbours now
        ties.update(s)
        #expect(await ties.settled())
        #expect(ties.settings?.maxGap == 3 && board.plate.ties.settings?.maxGap == 3 && ties.links.isEmpty)
        // what the printer cannot print is refused by the engine, named, and not applied
        var thin = try #require(ties.settings)
        thin.contactThickness = 0.2
        ties.update(thin)
        #expect(ties.settingsError?.contains("contactThickness") == true, "\(ties.settingsError ?? "")")
        #expect(ties.settings?.contactThickness == 0.6)
        var flat = try #require(ties.settings)
        flat.heightFrom = 0.9
        flat.heightTo = 0.5
        ties.update(flat)
        #expect(ties.settingsError?.contains("heightFrom") == true)
        ties.resetToProfile()
        #expect(await ties.settled())
        #expect(ties.settingsError == nil && ties.settings?.maxGap == 8 && ties.links.count == 2)
        let defaults = try #require(TieSettings.decode(try plateTiesSettings(profileId: PrinterProfileStore.shared.id, settingsJson: "")))
        #expect(ties.settings == defaults)
    }

    @Test func thePrinterProfileIsRememberedAndAnUnknownOneFallsBack() throws {
        let suite = "plate-ties-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = PrinterProfileStore(defaults: defaults)
        #expect(store.id == "elegoo-saturn-4-ultra-16k" && store.current?.isDefault == true)
        store.id = "no-such-printer"
        #expect(store.id == "elegoo-saturn-4-ultra-16k", "an unknown id is the default")
        defaults.set("no-such-printer", forKey: PrinterProfileStore.key)
        #expect(PrinterProfileStore(defaults: defaults).id == "elegoo-saturn-4-ultra-16k")
        defaults.set("elegoo-saturn-4-ultra-16k", forKey: PrinterProfileStore.key)
        #expect(PrinterProfileStore(defaults: defaults).id == "elegoo-saturn-4-ultra-16k")
    }

    // MARK: the lock

    @Test func tiesLockArrangeAndMoveAndOffUnlocksThem() async throws {
        let (model, board, ties) = try await coins([-7, 0, 7])
        defer { withExtendedLifetime(model) {} }
        let move = try #require(model.features[MoveFeature.self]?.model)
        move.setOn(true)
        #expect(move.isOn && board.canArrange && board.lockReason() == nil)
        try await on(ties)
        #expect(!move.isOn, "turning ties on stops Move")
        #expect(board.lockReason() != nil && !board.canArrange)
        #expect(model.features.statusLines.contains { $0.id == "plate-ties-lock" })
        move.setOn(true)
        #expect(!move.isOn && model.status == board.lockReason(), "Move says why it stays off")
        let before = board.plate.models.map(\.xy)
        model.status = nil
        await board.arrange()
        #expect(board.plate.models.map(\.xy) == before && model.status == board.lockReason(), "Arrange moves nothing and says why")
        ties.setOn(false)
        #expect(board.lockReason() == nil && board.canArrange)
        move.setOn(true)
        #expect(move.isOn)
        move.setOn(false)
    }

    // MARK: rebuild on change

    @Test func theTiesFollowThePlateWhenItsModelsOrFinsChange() async throws {
        let (model, board, ties) = try await coins([-7, 0, 7])
        defer { withExtendedLifetime(model) {} }
        try await on(ties)
        let gen = ties.generation
        // a parked coin moved away (an undo of a move does this): its link goes
        let far = try #require(board.plate.models.first { $0 !== board.plate.liveSlot && $0.xy.x == -7 })
        far.xy = SIMD2(-40, 0)
        board.sync()
        #expect(await ties.settled())
        #expect(ties.generation > gen && ties.links.count == 1)
        // nothing changed: no new run
        let g2 = ties.generation
        board.sync()
        #expect(ties.generation == g2 && !ties.working)
        // fins on: every coin gets fins, and the ties are built again with them in the way
        model.session.perform("Add Fins") { model.session.finsOn = true }
        #expect(await eventually { model.wiring?.build?.latest != nil && model.wiring?.build?.building == false })
        board.scheduleParked()
        let parked = board.plate.models.compactMap(\.stash)
        #expect(await eventually(60) { parked.allSatisfy { $0.build.latest != nil && !$0.build.building } }, "the parked coins' fins")
        board.sync()
        #expect(await ties.settled())
        #expect(ties.generation > g2 && ties.error == nil, "\(ties.error ?? "")")
        // the model taken off the plate: the coins left stand 40 mm apart, no neighbours
        board.removeModel()
        #expect(await ties.settled())
        #expect(ties.links.isEmpty)
    }

    @Test func eachPlateKeepsItsOwnTies() async throws {
        let (model, board, ties) = try await coins([-7, 0])
        defer { withExtendedLifetime(model) {} }
        try await on(ties)
        #expect(ties.links.count == 1)
        board.addPlate()
        #expect(!ties.state.on && ties.links.isEmpty && model.controller?.renderer.tieScene == nil, "a new plate has none")
        #expect(board.lockReason() == nil)
        board.switchPlate(0)
        #expect(ties.state.on && board.lockReason() != nil)
        #expect(await ties.settled())
        #expect(ties.links.count == 1)
    }

    // MARK: export and persistence

    func dir() throws -> URL {
        let d = FileManager.default.temporaryDirectory.appending(path: "plate-ties-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        return d
    }

    /// `<item` elements of a stored (uncompressed) 3MF: one per object the slicer places.
    func items(_ url: URL) throws -> Int {
        try String(decoding: Data(contentsOf: url), as: UTF8.self).components(separatedBy: "<item ").count - 1
    }

    @Test func aTiedPlateExportsAsOneObjectAndAnUntiedOneAsItsModels() async throws {
        let (model, board, ties) = try await coins([-7, 0, 7])
        defer { withExtendedLifetime(model) {} }
        let d = try dir()
        let plain = d.appending(path: "plain.3mf")
        #expect(await board.writePlate(try #require(board.plateWrite(0, deferStanding: true)), to: plain) == plain, "\(board.exportError ?? "")")
        #expect(try items(plain) == 3)
        #expect(try plateTiesOf3mf(path: plain.path) == nil)
        #expect(board.tieRequest(0) == nil, "untied: the plain export")
        try await on(ties)
        #expect(board.tieRequest(0) != nil, "tied: the merged export \(ties.error ?? "")")
        let tied = d.appending(path: "tied.3mf")
        #expect(await board.writePlate(try #require(board.plateWrite(0, deferStanding: true)), to: tied) == tied, "\(board.exportError ?? "")")
        #expect(try items(tied) == 1, "one merged object")
        let doc = try #require(try plateTiesOf3mf(path: tied.path))
        #expect(doc.contains("\"profile\":\"elegoo-saturn-4-ultra-16k\""))
    }

    @Test func allPlatesWritesEachTiedPlateMerged() async throws {
        let (model, board, ties) = try await coins([-7, 0])
        defer { withExtendedLifetime(model) {} }
        try await on(ties)
        board.addPlate()
        await model.load(url: try Self.coin("other"))
        let d = try dir()
        board.savePanel = PlateExportTests.Panel(url: d.appending(path: "support-fins-plates.zip"))
        let url = try #require(await board.exportAll(), "\(board.exportError ?? "")")
        let text = String(decoding: try Data(contentsOf: url), as: UTF8.self)
        #expect(text.contains("support_fins_plate_ties.json"), "the tied plate carries its ties")
        #expect(text.contains("plate-1-") && text.contains("plate-2-"))
    }

    @Test func aTiedThreeMFOpenedBringsItsTiesBack() async throws {
        let (model, board, ties) = try await coins([-7, 0, 7])
        defer { withExtendedLifetime(model) {} }
        try await on(ties)
        var s = try #require(ties.settings)
        s.maxGap = 6.5
        s.tiesPerPair = 2
        ties.update(s)
        ties.toggle(try #require(ties.links.first))
        #expect(await ties.settled())
        let url = try dir().appending(path: "saved.3mf")
        #expect(await board.writePlate(try #require(board.plateWrite(0, deferStanding: true)), to: url) == url, "\(board.exportError ?? "")")
        // opened in a new window: one merged object, its ties on with the settings and links it was saved with
        let other = ViewerModel()
        await other.load(url: url)
        let back = try #require(other.features[PlateTiesFeature.self]?.model)
        #expect(await eventually { back.state.on })
        #expect(back.settings?.maxGap == 6.5 && back.settings?.tiesPerPair == 2)
        #expect(back.state.disabled.count == 1)
        let ob = try #require(other.features[PlatesFeature.self]?.board)
        #expect(ob.lockReason() != nil && ob.plate.ties.on)
        // the document round-trips: the same settings written back
        #expect(await back.settled())
        let doc = try #require(try plateTiesOf3mf(path: url.path))
        // read with the file: the part hands the document over, no second read (#245 decision 6)
        #expect(other.session.parts.first?.part.savedPlateTies() == doc)
        back.apply(document: doc)
        #expect(back.settings?.maxGap == 6.5)
    }

    // MARK: links by model name (decision C)

    @Test func tieKeysAreModelNamesCountedInPlateOrder() {
        #expect(TieKey.keys(["coin", "bracket", "coin", nil, "coin#2", "coin"]) == ["coin#1", "bracket#1", "coin#2", nil, "coin#2#1", "coin#3"])
        #expect(TieKey.name("coin#2#1") == "coin#2" && TieKey.name("bracket#1") == "bracket" && TieKey.name("odd") == "odd")
        // no name can take another model's key
        let keys = TieKey.keys(["a", "a", "a#1", "a#2", "a#1"]).compactMap { $0 }
        #expect(Set(keys).count == keys.count, "\(keys)")
    }

    @Test func aPlatesLinksNameItsModels() async throws {
        let a = try Self.coin("twin"), b = try Self.coin("other")
        let (model, board, ties) = try await coins([-7, 0, 7], files: [a, b, a])
        defer { withExtendedLifetime(model) {} }
        let (twin, other) = (a.deletingPathExtension().lastPathComponent, b.deletingPathExtension().lastPathComponent)
        #expect(board.tieKeys(0) == ["\(twin)#1", "\(other)#1", "\(twin)#2"])
        #expect(board.plateWrite(0)?.keys == ["\(twin)#1", "\(other)#1", "\(twin)#2"])
        try await on(ties)
        #expect(ties.links.map { [$0.a, $0.b] } == [["\(twin)#1", "\(other)#1"], ["\(other)#1", "\(twin)#2"]])
        #expect(ties.name("\(twin)#2") == twin)
        ties.toggle(try #require(ties.links.last))
        #expect(ties.state.disabled == [TiePairKey(a: "\(other)#1", b: "\(twin)#2")])
        #expect(try #require(ties.document).contains("\"disabled\":[[\"\(other)#1\",\"\(twin)#2\"]]"))
    }

    @Test func aDocumentBringsItsLinksBackOnAPlateOfTheSameModels() async throws {
        let files = try (0..<3).map { try Self.coin("same\($0)") }
        let (first, _, ties) = try await coins([-7, 0, 7], files: files)
        defer { withExtendedLifetime(first) {} }
        try await on(ties)
        ties.toggle(try #require(ties.links.first))
        #expect(await ties.settled())
        #expect(ties.links.count == 1)
        let doc = try #require(ties.document)
        // another window, the same models (new slots, new ids): the link clicked off is off again
        let (second, _, back) = try await coins([-7, 0, 7], files: files)
        defer { withExtendedLifetime(second) {} }
        back.apply(document: doc)
        #expect(await back.settled())
        #expect(back.error == nil, "\(back.error ?? "")")
        #expect(back.candidates.count == 2 && back.links.count == 1, "\(back.links.map(\.a))")
        #expect(back.links.first.map { [$0.a, $0.b] } == ties.links.first.map { [$0.a, $0.b] })
    }

    // MARK: a tied 3MF written again (decision D)

    @Test func aReopenedTiedThreeMFExportedAgainKeepsItsDocument() async throws {
        let (model, board, ties) = try await coins([-7, 0, 7])
        defer { withExtendedLifetime(model) {} }
        try await on(ties)
        var s = try #require(ties.settings)
        s.maxGap = 6.5
        ties.update(s)
        ties.toggle(try #require(ties.links.first))
        #expect(await ties.settled())
        let d = try dir()
        let saved = d.appending(path: "saved.3mf")
        #expect(await board.writePlate(try #require(board.plateWrite(0, deferStanding: true)), to: saved) == saved, "\(board.exportError ?? "")")
        let doc = try #require(try plateTiesOf3mf(path: saved.path))
        // opened again: one model, written by the one-model export
        let other = ViewerModel()
        await other.load(url: saved)
        let back = try #require(other.features[PlateTiesFeature.self]?.model)
        #expect(await eventually { back.state.on })
        let exporter = try #require(other.wiring?.exporter)
        let again = d.appending(path: "again.3mf")
        #expect(await exporter.write(.threeMF, to: again) == again, "\(exporter.error ?? "")")
        #expect(try plateTiesOf3mf(path: again.path) == doc, "the same printer, settings and links")
        // ties off: the one-model 3MF has none
        back.setOn(false)
        let off = d.appending(path: "off.3mf")
        #expect(await exporter.write(.threeMF, to: off) == off, "\(exporter.error ?? "")")
        #expect(try plateTiesOf3mf(path: off.path) == nil)
    }

    // MARK: the printer's bed and layer (decision A)

    @Test func pickingAPrinterSetsTheBedAndTheLayerHeight() async throws {
        let suite = "plate-ties-printer-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = PrinterProfileStore(defaults: defaults)
        let saturn = try #require(store.current)
        #expect(store.picks == 0)
        store.choose(saturn.id)
        store.choose(saturn.id)
        #expect(store.picks == 2 && store.id == saturn.id, "the printer shown, picked again, counts")

        let (model, board, _) = try await coins([-7, 7])
        defer { withExtendedLifetime(model) {} }
        let feature = try #require(model.features[PlateTiesFeature.self])
        let (volumes, settings) = (VolumeStore(defaults: defaults), SettingsStore(defaults: defaults))
        feature.useProfile(saturn, volumes: volumes, store: settings)
        let bed = PrinterVolume(x: 211.68, y: 118.37, z: 220)
        #expect(model.controller?.volume == bed)
        #expect(model.session.settings.layerHeight == 0.05)
        // remembered: the next window starts with them
        #expect(volumes.load() == bed)
        #expect(settings.load().layerHeight == 0.05)
        // the Printer section's line and the plate's export bed follow
        #expect(model.session.settings.printerSummary.contains("0.05 layers"), "\(model.session.settings.printerSummary)")
        #expect(board.bedOptions == "{\"bed\":[211.68,118.37]}")
        #expect(model.wiring?.exporter?.optionsJSON(for: .threeMF)?.contains("\"bed\":[211.6") == true)
        // one undo step
        model.session.undo()
        #expect(model.session.settings.layerHeight != 0.05)
        model.session.redo()
        // changed by hand afterwards: the printer stays picked, the hand-set values hold
        model.controller?.volume = PrinterVolume.standard
        var s = model.session.settings
        s.layerHeight = 0.1
        model.session.setSettings(s)
        #expect(store.id == saturn.id && model.session.settings.layerHeight == 0.1 && model.controller?.volume == .standard)
    }

    /// The UI run of `testPickingThePrinterSetsTheBedAndTheLayerHeight` timed out waiting for "0.1 layers" after 0.1 was
    /// typed over the printer's 0.05: the value path, headless. The field reads "0.1" as 0.1, keeps it (no snapping to
    /// the 0.04 step from 0.05 or from the 0.01 minimum), and the Printer row says so.
    @Test func aLayerTypedOverThePrintersIsKeptAsTyped() async throws {
        let limits = FinSettings.layerHeightLimits
        let style = NumberField.format.locale(Locale(identifier: "en_US"))
        #expect(style.format(0.05) == "0.05")
        for (typed, want) in [("0.1", 0.1), ("0.2", 0.2), ("0.05", 0.05), ("0.09", 0.09), ("0.13", 0.13), ("0.005", 0.01), ("0.7", 0.6)] {
            let parsed = try style.parseStrategy.parse(typed)
            #expect(NumberField.clamp(parsed, to: limits) == want, "\(typed)")
        }
        let (model, _, _) = try await coins([-7, 7])
        defer { withExtendedLifetime(model) {} }
        let feature = try #require(model.features[PlateTiesFeature.self])
        let suite = "plate-ties-layer-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        feature.useProfile(try #require(PrinterProfileStore(defaults: defaults).current),
                           volumes: VolumeStore(defaults: defaults), store: SettingsStore(defaults: defaults))
        #expect(model.session.settings.printerSummary.contains("0.05 layers"))
        var s = model.session.settings
        s.layerHeight = NumberField.clamp(try style.parseStrategy.parse("0.1"), to: limits)
        model.session.setSettings(s)
        #expect(model.session.settings.layerHeight == 0.1)
        #expect(model.session.settings.printerSummary.contains("0.1 layers"), "\(model.session.settings.printerSummary)")
    }

    @Test func everyPrinterLayerFitsTheLayerHeightField() {
        for p in printerProfiles() {
            #expect(FinSettings.layerHeightLimits.range.contains(p.layerHeightDefault), "\(p.id)")
            #expect(p.layerHeight == p.layerHeightDefault)
            #expect(p.volume != nil, "\(p.id)")
        }
    }
}
