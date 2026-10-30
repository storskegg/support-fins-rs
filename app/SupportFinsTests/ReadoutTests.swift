import Foundation
import SupportFinsCore
import Testing
@testable import Support_Fins

/// The status readout against the web's own `ui/readout.js`: `parity/corpus/readout/*.jsonl`, recorded by
/// `parity/oracle/readout.js` (the real file, run in Deno with the page's state stood in).
@Suite("Readout")
struct ReadoutTests {
    private static var root: URL {
        URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    }

    private struct Recorded {
        let id: String
        let args: [Any]
        let out: [String: Any]
    }

    private static func corpus(_ name: String) throws -> [Recorded] {
        let text = try String(contentsOf: root.appending(path: "parity/corpus/readout/\(name).jsonl"), encoding: .utf8)
        return try text.split(separator: "\n").map { line in
            let obj = try #require(try JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any])
            return Recorded(id: try #require(obj["id"] as? String), args: try #require(obj["args"] as? [Any]),
                            out: obj["out"] as? [String: Any] ?? ["value": obj["out"] as Any])
        }
    }

    private func decode<T: Decodable>(_ type: T.Type, _ any: Any) throws -> T {
        try JSONDecoder().decode(type, from: JSONSerialization.data(withJSONObject: any))
    }

    @Test func everyRecordedReadoutMatchesTheWeb() throws {
        let cases = try Self.corpus("updateReadout")
        #expect(cases.count >= 70)
        for c in cases {
            let built = try decode(ReadoutBuilt.self, c.args[0])
            let ctx = try decode(ReadoutContext.self, c.args[1])
            let got = FinReadout.make(built, ctx)
            let fins = try #require(c.out["fins"] as? [String: Any])
            #expect(got.fins == fins["text"] as? String, "\(c.id): fins")
            #expect(got.finsWarn == fins["warn"] as? Bool, "\(c.id): warn")
            #expect(got.pad == c.out["pad"] as? String, "\(c.id): pad")
            #expect(got.lead == c.out["lead"] as? String, "\(c.id): lead")
            let info = try #require(c.out["info"] as? [String: Any])
            #expect(got.info == info["title"] as? String, "\(c.id): info")
            #expect(got.info.isEmpty == (info["hidden"] as? Bool), "\(c.id): info hidden")
            // the timing line is left alone when the build time is absent (the web's text stays as it was)
            if ctx.ms != nil { #expect(got.time == c.out["time"] as? String, "\(c.id): time") } else { #expect(got.time == nil, "\(c.id): time") }
            let receipt = try #require(c.out["receipt"] as? [String: Any])
            #expect((got.grams == nil) == (receipt["hidden"] as? Bool), "\(c.id): receipt hidden")
            if let g = got.grams { #expect(g == receipt["grams"] as? String, "\(c.id): grams") }
        }
    }

    @Test func gramsAreRoundedAsTheWebRoundsThem() throws {
        let cases = try Self.corpus("fmtGrams")
        #expect(cases.count >= 15)
        for c in cases {
            let g = try #require(c.args[0] as? Double)
            #expect(FinReadout.fmtGrams(g) == c.out["value"] as? String, "\(c.id)")
        }
    }

    @Test func toFixed1MatchesV8OnTies() throws {
        let cases = try Self.corpus("toFixed1")
        #expect(cases.count >= 15)
        for c in cases {
            let x = try #require(c.args[0] as? Double)
            #expect(JS.toFixed1(x) == c.out["value"] as? String, "\(c.id)")
        }
    }

    @Test func noBuildYetIsDashes() {
        let r = FinReadout.make(nil, ReadoutContext())
        #expect(r.fins == "—" && r.pad == "—" && r.lead.isEmpty && !r.finsWarn && r.time == nil && r.grams == nil)
    }

    @Test func jsRoundingAndToFixedKeepTheirTies() {
        #expect(JS.round(12.5) == 13 && JS.round(0.49999999999999994) == 0 && JS.round(2.4) == 2 && JS.round(2.5) == 3)
        #expect(JS.toFixed0(12.5) == "13" && JS.toFixed0(0.5) == "1" && JS.toFixed0(12.4) == "12" && JS.toFixed0(0) == "0")
        #expect(JS.toFixed1(5.55) == "5.5") // 5.55 is 5.5499999999999998 in binary
        #expect(JS.tn("{n} tine", "{n} tines", 1) == "1 tine" && JS.tn("{n} tine", "{n} tines", 0) == "0 tines")
    }
}

/// The part rows (`ui/part.js`), checked against the strings the JS builds (cited per case).
@Suite("Part readout")
struct PartReadoutTests {
    private func input(_ build: (inout PartReadoutInput) -> Void) -> PartReadoutInput {
        var i = PartReadoutInput()
        i.name = "cube.stl"
        i.triangles = 12
        i.size = (20, 20.04, 19.96)
        i.analysisMs = 12.5
        i.weldMs = 0.4
        build(&i)
        return i
    }

    private func text(_ segs: [OverhangsSegment]) -> String {
        segs.map { if case .text(let t) = $0 { t } else { "●" } }.joined()
    }

    @Test func theOverhangsRowCountsRegionsSliversAndSmallHoles() {
        var r = PartReadout.make(input { $0.regionCount = 0; $0.rawRegionCount = 0 })
        #expect(text(r.overhangs) == "none" && r.overhangsGood) // part.js:307, :326
        r = PartReadout.make(input { $0.regionCount = 1; $0.rawRegionCount = 1 })
        #expect(text(r.overhangs) == "1 region" && !r.overhangsGood)
        r = PartReadout.make(input { $0.regionCount = 3; $0.rawRegionCount = 5 })
        #expect(text(r.overhangs) == "3 regions (+●2 slivers)") // :316-318
        r = PartReadout.make(input { $0.regionCount = 0; $0.rawRegionCount = 1 })
        #expect(text(r.overhangs) == "none (+●1 sliver)" && !r.overhangsGood, "slivers alone are not 'none to support'")
        r = PartReadout.make(input { $0.regionCount = 2; $0.rawRegionCount = 2; $0.smallHoleCount = 1 })
        #expect(text(r.overhangs) == "2 regions (●1 small hole left for bridging)") // :321-324
        r = PartReadout.make(input { $0.regionCount = 2; $0.rawRegionCount = 3; $0.smallHoleCount = 2 })
        #expect(text(r.overhangs) == "2 regions (+●1 sliver) (●2 small holes left for bridging)")
        if case .swatch(let help) = r.overhangs[2] { #expect(help == "Amber: overhangs under 12 mm², too small for a fin") } else { Issue.record("no swatch") }
    }

    @Test func theOverhangsRowCountsMidairStartsAndStopsBeingGreen() {
        var r = PartReadout.make(input { $0.regionCount = 0; $0.rawRegionCount = 0; $0.islandCount = 1 })
        #expect(text(r.overhangs) == "none (+●1 midair start)" && !r.overhangsGood) // islands.js:114-126
        r = PartReadout.make(input { $0.regionCount = 2; $0.rawRegionCount = 3; $0.smallHoleCount = 1; $0.islandCount = 3 })
        #expect(text(r.overhangs) == "2 regions (+●1 sliver) (●1 small hole left for bridging) (+●3 midair starts)")
        if case .redSwatch(let help) = r.overhangs[r.overhangs.count - 2] { #expect(help == "Spots that start in midair") } else { Issue.record("no red swatch") }
        r = PartReadout.make(input { $0.regionCount = 0; $0.rawRegionCount = 0; $0.islandCount = 0 })
        #expect(r.overhangsGood)
    }

    @Test func areaBedSizeAndTrianglesAreRoundedAsTheWebRoundsThem() {
        let r = PartReadout.make(input { $0.overArea = 1234.5; $0.bedArea = 0.5; $0.triangles = 1_234_567 })
        #expect(r.area == "1235 mm²" && r.bed == "1 mm²") // toFixed(0): a tie takes the larger (part.js:366-367)
        #expect(r.bedWarn, "0.5 mm² is under 1")
        #expect(r.size == "20.0 × 20.0 × 20.0 mm") // 20.04 and 19.96 round to 20.0 (part.js:449)
        #expect(r.triangles == 1_234_567.formatted())
        #expect(PartReadout.make(input { $0.bedArea = 0.99 }).bedWarn) // part.js:368
        #expect(PartReadout.make(input { $0.bedArea = 0 }).bed == "0 mm²")
        #expect(r.time == "13 ms · weld 0 ms") // 12.5 -> 13, 0.4 -> 0 (part.js:390)
    }

    @Test func theFlatNoteSaysWhetherYouNeedSupportsAtAll() {
        var r = PartReadout.make(input { $0.regionCount = 0; $0.rawRegionCount = 0 })
        #expect(r.flatNote == "No supports needed this way up." && r.flatNoteGood) // part.js:377-378
        r = PartReadout.make(input { $0.regionCount = 0; $0.rawRegionCount = 2 })
        #expect(r.flatNote == "Nothing big enough for a fin this way up." && r.flatNoteGood)
        r = PartReadout.make(input { $0.regionCount = 0; $0.islandCount = 1 })
        #expect(r.flatNote == "", "a start in midair: the midair-starts card speaks (part.js:374-376)")
        r = PartReadout.make(input { $0.regionCount = 2; $0.rawRegionCount = 2; $0.flatRegions = 0 })
        #expect(r.flatNote == "This prints clean lying flat. You only need fins if you’re tilting it for strength." && !r.flatNoteGood)
        r = PartReadout.make(input { $0.regionCount = 2; $0.rawRegionCount = 2; $0.flatRegions = 2 })
        #expect(r.flatNote == "")
        r = PartReadout.make(input { $0.regionCount = 2; $0.rawRegionCount = 2; $0.flatRegions = nil })
        #expect(r.flatNote == "")
    }

    @Test func theSliverWarningNeedsHighlightSmallOverhangs() {
        var r = PartReadout.make(input { $0.regionCount = 1; $0.rawRegionCount = 2 })
        #expect(r.overWarn == "⚠ 1 overhang shaded amber is too small for a fin (under 12 mm² each), so it prints unsupported this way up and may come out rough. Try Suggest orientation to point it up.")
        r = PartReadout.make(input { $0.regionCount = 1; $0.rawRegionCount = 4 })
        #expect(r.overWarn == "⚠ 3 overhangs shaded amber are too small for a fin (under 12 mm² each), so they print unsupported this way up and may come out rough. Try Suggest orientation to point them up.")
        r = PartReadout.make(input { $0.regionCount = 1; $0.rawRegionCount = 4; $0.highlightSmall = false })
        #expect(r.overWarn == "")
        r = PartReadout.make(input { $0.regionCount = 1; $0.rawRegionCount = 1 })
        #expect(r.overWarn == "")
    }

    @Test func fitCountsTheFinsAndWhereTheModelStands() {
        func fit(_ build: (inout FitInput) -> Void) -> FitReadout {
            var f = FitInput()
            f.size = (100, 100, 50)
            build(&f)
            return FitReadout.make(f)
        }
        #expect(fit { _ in } == FitReadout(text: "fits", warn: false)) // part.js:502-503
        #expect(fit { $0.volume = (90, 256, 256) } == FitReadout(text: "does not fit", warn: true))
        #expect(fit { $0.volume = (256, 256, 40) } == FitReadout(text: "does not fit", warn: true))
        // the pad spreads past the part: it fits alone and not with the fins
        #expect(fit { $0.volume = (110, 256, 256); $0.added = (-60, 60, -50, 50, 10) } == FitReadout(text: "does not fit (with fins)", warn: true))
        #expect(fit { $0.volume = (130, 256, 256); $0.added = (-60, 60, -50, 50, 10) } == FitReadout(text: "fits", warn: false))
        #expect(fit { $0.added = (-50, 50, -50, 50, 60); $0.volume = (256, 256, 55) } == FitReadout(text: "does not fit (with fins)", warn: true))
        // hanging off the plate
        #expect(fit { $0.spot = (100, 0); $0.volume = (200, 200, 200) } == FitReadout(text: "hangs off the bed", warn: true))
        #expect(fit { $0.spot = (50, 0); $0.volume = (200, 200, 200) } == FitReadout(text: "fits", warn: false), "exactly on the edge fits (1e-6)")
        #expect(fit { $0.parkedOff = true } == FitReadout(text: "hangs off the bed", warn: true))
        #expect(fit { $0.clash = true } == FitReadout(text: "models overlap", warn: true))
        #expect(fit { $0.clash = true; $0.parkedOff = true } == FitReadout(text: "models overlap · hangs off the bed", warn: true))
        #expect(fit { $0.clash = true; $0.volume = (90, 90, 90) } == FitReadout(text: "does not fit", warn: true), "too big outranks the rest")
    }
}

@Suite("Readout text from the UI map")
struct ReadoutUIMapTests {
    @Test func theReceiptTooltipIsTheWebsText() throws {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let text = try String(contentsOf: root.appending(path: "docs/UI-MAP.md"), encoding: .utf8)
        let line = try #require(text.split(separator: "\n").first { $0.hasPrefix("| `r-info`") })
        let cells = line.split(separator: "|", omittingEmptySubsequences: false).dropFirst().map { $0.trimmingCharacters(in: .whitespaces) }
        #expect(cells[5] == FinReadout.receiptTip)
    }
}


/// The readout against a REAL engine build: the summary JSON the Rust engine produces must decode into what the
/// readout reads, and the text must say what that build did.
@MainActor
@Suite("Readout from a real build")
struct ReadoutFromEngineTests {
    /// A binary STL of a T: a 10 x 10 stem under a 40 x 40 slab, so the slab's underside is an overhang.
    private func tShapePath() throws -> String {
        try TestParts.write(TestParts.box(-5, 5, -5, 5, 0, 20) + TestParts.box(-20, 20, -20, 20, 20, 24),
                            to: Scratch.file("readout-tests-tshape.stl"))
    }

    @Test func theNewEngineFieldsReachTheReadouts() async throws {
        let part = try #require(try loadParts(path: try tShapePath()).first)
        let identity: [Double] = [1, 0, 0, 0, 1, 0, 0, 0, 1]
        // the part's side: raw regions, the analysis time, the weld time and the midair starts
        let a = try part.analyze(thresholdDeg: 45, rot: identity)
        #expect(a.rawRegionCount >= a.regionCount && a.regionCount > 0 && a.analyzeMs >= 0)
        #expect(part.info().weldMs >= 0)
        _ = try part.islands(thresholdDeg: 45, rot: identity)
        // the build's side: the pad, the extent and the reach are the engine's. (Flat contacts, which gives the walls a
        // crest, travels in the tunables: its build test comes with #112, which adds `tunables_json` to the model.)
        let model = BuildModel(part: part)
        model.build(rotation: identity, optionsJSON: FinSettings().optionsJSON())
        await model.settled()
        #expect(model.error == nil, "\(model.error ?? "")")
        let built = try ReadoutBuilt(summaryJSON: try #require(model.latest).summaryJson(), finCount: model.records?.fins.count ?? 0)
        let box = try #require(built.addedBounds, "the fins have an extent")
        #expect(box.min.count == 3 && box.max.count == 3 && box.max[2] > box.min[2] && box.min[0] < box.max[0])
        #expect(!model.readoutHasCrest, "no crest with the interface off")
        if built.bare != nil { #expect(built.fillReach != nil, "the reach comes with bare") }
        if let pad = built.pad { #expect(["auto", "light", "sure", "custom"].contains(pad.style)) }
        // the fit says what the fins add: it is wider than the bare part
        let size = a.size
        let fit = FitReadout.make(FitInput(size: (size[0], size[1], size[2]),
                                           added: (box.min[0], box.max[0], box.min[1], box.max[1], box.max[2])))
        #expect(!fit.text.isEmpty)
    }

    @Test func flatContactsGiveTheWallTopsACrest() async throws {
        let part = try #require(try loadParts(path: try tShapePath()).first)
        let model = BuildModel(part: part)
        var s = FinSettings()
        s.interfaceMaterial = .flat
        model.build(rotation: [1, 0, 0, 0, 1, 0, 0, 0, 1], optionsJSON: s.optionsJSON(), tunablesJSON: s.tunablesJSON())
        await model.settled()
        #expect(model.error == nil, "\(model.error ?? "")")
        #expect(model.readoutHasCrest, "crestRanges is not empty with Flat contacts on")
        s.interfaceMaterial = .off
        model.build(rotation: [1, 0, 0, 0, 1, 0, 0, 0, 1], optionsJSON: s.optionsJSON(), tunablesJSON: s.tunablesJSON())
        await model.settled()
        #expect(!model.readoutHasCrest)
    }

    @Test func removingAFinRemeasuresTheReadoutsAndFullBuildsTellTheIslandCard() async throws {
        let part = try #require(try loadParts(path: try tShapePath()).first)
        let identity: [Double] = [1, 0, 0, 0, 1, 0, 0, 0, 1]
        let model = BuildModel(part: part)
        model.build(rotation: identity, optionsJSON: FinSettings().optionsJSON(), tunablesJSON: FinSettings().tunablesJSON())
        await model.settled()
        let all = try ReadoutBuilt(summaryJSON: try #require(model.latest).summaryJson(), finCount: model.records?.fins.count ?? 0)
        #expect(all.islandsBuild == nil, "an Auto build tells the card nothing")
        // one fin removed: the ids come from the keys, the numbers from the engine
        let key = try #require(model.keys.first?.sig)
        let ids = model.readoutRemovedIDs([key])
        #expect(ids.count == 1)
        let text = try #require(model.readoutResult).afterRemovalJson(removedIds: ids)
        let after = try JSONDecoder().decode(ReadoutSections.AfterRemoval.self, from: Data(text.utf8))
        #expect(after.removed == 1)
        #expect(after.addedVolumeMm3 < all.finVolumeMm3 + all.padVolumeMm3, "the kept fins hold less")
        // a Full build reports islandsBuild, which the card reads
        var s = FinSettings()
        s.mode = .full
        model.build(rotation: identity, optionsJSON: s.optionsJSON(), tunablesJSON: s.tunablesJSON())
        await model.settled()
        #expect(model.error == nil, "\(model.error ?? "")")
        let full = try ReadoutBuilt(summaryJSON: try #require(model.latest).summaryJson(), finCount: model.records?.fins.count ?? 0)
        #expect(full.islandsBuild != nil, "a Full build carries islandsBuild")
        let bare = full.islandsBuild.map { $0.all == true ? UInt64(2) : UInt64($0.bare ?? 0) }
        // two starts: a card unless Full walled every one; without a Full build it always warns
        #expect(islandCard(count: 2, bare: bare, capped: false, markersVisible: true).isEmpty == (bare == 0))
        #expect(!islandCard(count: 2, bare: nil, capped: false, markersVisible: true).isEmpty)
    }

    @Test func theEnginesSummaryReadsAsFinsAndGrams() async throws {
        let part = try #require(try loadParts(path: try tShapePath()).first)
        let model = BuildModel(part: part)
        model.build(rotation: [1, 0, 0, 0, 1, 0, 0, 0, 1], optionsJSON: "{}")
        await model.settled()
        #expect(model.error == nil)
        let result = try #require(model.latest)
        let records = try #require(model.records)
        let built = try ReadoutBuilt(summaryJSON: result.summaryJson(), finCount: records.fins.count)
        #expect(built.finCount > 0)
        var ctx = ReadoutContext()
        ctx.regions = 1
        ctx.ms = 7
        ctx.analysisTiming = "1 ms · weld 1 ms"
        ctx.hasAdded = true
        ctx.volumeMm3 = built.finVolumeMm3 + built.padVolumeMm3
        let r = FinReadout.make(built, ctx)
        #expect(r.fins.contains("support fin") || r.fins.contains("prop"), "\(r.fins)")
        #expect(!r.finsWarn)
        #expect(r.time == "1 ms · weld 1 ms · fins 7 ms")
        let grams = try #require(r.grams)
        #expect(grams.hasSuffix(" g") && built.finVolumeMm3 > 0)
        #expect(!r.info.isEmpty, "the how-it-works text goes behind the (i)")
        #expect(r.pad == (built.pad == nil ? "not needed" : "added"))
    }
}
