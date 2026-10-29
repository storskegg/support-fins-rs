import Foundation
import Testing

@testable import Support_Fins

/// The settings search against the rules of `web/ui/search.js`: names first (as shown and in English), tooltips only
/// when no name matches, a match the mode hides stays hidden and its switch is shown instead, sections hold or hide.
@Suite("Settings search")
struct SettingsSearchTests {
    private func run(_ query: String, _ ctx: SearchContext = SearchContext()) -> SearchEngine.Result? {
        var engine = SearchEngine()
        return engine.run(query: query, ctx)
    }

    @Test func anEmptyBoxLeavesThePanelAsItWas() {
        #expect(run("") == nil)
        #expect(run("  \t ") == nil)
    }

    @Test func everyWordMustAppearInTheName() {
        // Tines' own name, "Tines grip the part", has both words too
        #expect(run("tine grip")?.show == ["tinegrip-fld", "tines"])
        #expect(run("grip tine")?.show == ["tinegrip-fld", "tines"])
        #expect(run("tine grip firm")?.show == ["tinegrip-fld"])
        // "light" is in Tine grip's "light ⟶ firm", in Bed pad's menu, and inside "Highlight"
        #expect(run("light")?.show == ["tinegrip-fld", "bed-pad", "highlight-small"])
    }

    @Test func aNameIncludesTheOptionsOfItsMenuAndIsFoldedForCaseAndAccents() {
        #expect(run("DIAMOND")?.show == ["cutout"])
        #expect(run("flat")?.show == ["iface"])
        #expect(run("sure")?.show == ["bed-pad"])
        // a match the mode hides, with no switch to show, stays hidden: nothing shows
        let hidden = run("SEPARATE")
        #expect(hidden?.show == ["separate-walls-fld"] && hidden?.none == true)
        #expect(SearchEngine.fold("Éclair Ñ") == "eclair n")
    }

    @Test func gapFindsSupportGapAndPadGapAndShowsTheSwitchThatRevealsThePad() {
        let r = run("gap")
        #expect(r?.show == ["gap", "pad-gap", "bed-pad"])
        #expect(r?.blocks == [.printer, .more, .holdThePart])
        #expect(r?.none == false)
        // in Custom the pad's number is a row of its own
        var ctx = SearchContext()
        ctx.settings.bedPad = .custom
        let custom = run("gap", ctx)
        #expect(custom?.show == ["gap", "pad-gap"])
    }

    @Test func tooltipsAreTheFallbackOnlyWhenNoNameMatches() {
        // "amber" is only in Highlight small overhangs' tooltip
        #expect(run("amber")?.show == ["highlight-small"])
        // "clearance" is in tooltips of several rows, but no name has it: all of them match
        let r = run("clearance")
        #expect(r?.show.contains("gap") == true && r?.show.contains("pad-gap") == true)
        // a name match hides the tooltip matches: "gap" is in the tooltips of others, which do not show
        #expect(run("gap")?.show.contains("pad-grip") == false)
    }

    @Test func nothingMatching() {
        let r = run("zzzz")
        #expect(r?.none == true && r?.show.isEmpty == true && r?.blocks.isEmpty == true)
    }

    @Test func aMatchTheModeHidesStaysHiddenAndItsSwitchShows() {
        var ctx = SearchContext()
        // sway off: Brace depth and Full brace depth are hidden; the Sway braces switch shows
        let off = run("brace depth", ctx)
        #expect(off?.show == ["sway-depth-fld", "full-brace-depth-fld", "sway"])
        #expect(off?.blocks == [.more, .tallParts])
        // sway on: they are rows
        ctx.settings.sway = true
        #expect(run("brace depth", ctx)?.show == ["sway-depth-fld", "full-brace-depth-fld"])
        // the brace tine fields wait on Tines too: with sway on and Tines off the switch is Tines
        ctx.settings.tines = false
        let noTines = run("brace tine spacing", ctx)
        #expect(noTines?.show == ["sway-spacing-fld", "tines"])
        // Cross-rib height: Brace tall walls (sway on), or Sway braces (off)
        ctx.settings.tines = true
        ctx.settings.tallRibs = false
        #expect(run("cross-rib", ctx)?.show == ["sway-cross-height-fld", "tall-ribs"])
        ctx.settings.sway = false
        #expect(run("cross-rib", ctx)?.show == ["sway-cross-height-fld", "sway"])
    }

    @Test func layerHeightAndTheGripSliderAreRevealedByTines() {
        var ctx = SearchContext()
        ctx.settings.tines = false
        ctx.settings.bedPad = .off
        ctx.settings.interfaceMaterial = .off
        #expect(run("layer height", ctx)?.show == ["layerh-fld", "tines"])
        #expect(run("tine grip", ctx)?.show == ["tinegrip-fld", "tines"])
    }

    @Test func coverageAndTheDrawButtonsWaitOnPlacement() {
        var ctx = SearchContext()
        ctx.settings.mode = .draw      // Wide-face coverage is Auto's: hidden in Draw
        #expect(run("coverage", ctx)?.show == ["coverage-fld", "fin-mode-fld"])
        // Clear all outside Draw: the Placement row shows, so the mode can be switched
        ctx.settings.mode = .auto
        #expect(run("clear all", ctx)?.show == ["draw-clear", "fin-mode-fld"])
        // in Draw, with its controls on show, it is a row
        ctx.settings.mode = .draw
        ctx.drawControls = true
        #expect(run("clear all", ctx)?.show == ["draw-clear"])
        // Restore all waits on Remove fins, which waits on Placement
        var rm = SearchContext()
        #expect(run("restore", rm)?.show == ["restore-fins", "remove-fins-toggle"])
        #expect(run("remove fins", rm)?.show == ["remove-fins-toggle", "fin-mode-fld"])
        rm.removeControls = true
        rm.canRestore = true
        #expect(run("restore", rm)?.show == ["restore-fins"])
    }

    @Test func aGateStaysOnceItRevealedAMatchWhileTheQueryIsTheSame() {
        var engine = SearchEngine()
        var ctx = SearchContext()
        _ = engine.run(query: "brace depth", ctx)
        ctx.settings.sway = true        // the user ticked it
        let after = engine.run(query: "brace depth", ctx)
        #expect(after?.show == ["sway-depth-fld", "full-brace-depth-fld", "sway"], "the switch stays on show to switch back off")
        // another query forgets it
        #expect(engine.run(query: "brace d", ctx)?.show == ["sway-depth-fld", "full-brace-depth-fld"])
        #expect(engine.gated.isEmpty)
    }

    @Test func theGripSliderIsPinsPerWallForVerticalTines() {
        var ctx = SearchContext()
        #expect(run("pins per wall", ctx)?.none == true, "Tine grip is the name while the tines are horizontal")
        ctx.settings.tineDirection = .vertical
        #expect(run("pins per wall", ctx)?.show == ["tinegrip-fld"])
        #expect(run("tine grip", ctx)?.show == ["tines"])
    }

    @Test func aSectionWithoutAMatchIsHiddenAndOneWithAMatchIsOpened() {
        let r = run("cutouts")
        #expect(r?.show == ["cutout"] && r?.blocks.isEmpty == true, "Cutouts sits in no folded section")
        let n = run("nozzle")
        #expect(n?.blocks == [.printer])
    }

    // MARK: against the page

    private struct Step {
        var state: SearchContext
        var query: String
        var hits: Set<String>
        var nohit: Set<String>
        var empty: Bool
        var searching: Bool
    }

    /// `parity/corpus/search/cases.json`, recorded by `parity/oracle/search.js` from the real `web/ui/search.js` on
    /// the real options panel: for each window state and query, the rows it marked and the sections it hid.
    private static func corpus() throws -> [(id: String, steps: [Step])] {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let data = try Data(contentsOf: root.appending(path: "parity/corpus/search/cases.json"))
        let top = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        let cases = try #require(top["cases"] as? [[String: Any]])
        return try cases.map { c in
            let steps = try #require(c["steps"] as? [[String: Any]]).map { s -> Step in
                let state = try #require(s["state"] as? [String: Any])
                var ctx = SearchContext()
                if let settings = state["settings"] {
                    ctx.settings = try JSONDecoder().decode(FinSettings.self, from: JSONSerialization.data(withJSONObject: settings))
                }
                let ui = state["ui"] as? [String: Bool] ?? [:]
                ctx.augmentOffered = ui["augment"] ?? false
                ctx.removeControls = ui["remove"] ?? false
                ctx.canRestore = ui["restore"] ?? false
                ctx.drawControls = ui["draw"] ?? false
                ctx.drawHint = ui["hint"] ?? false
                ctx.drawStarted = ui["started"] ?? false
                ctx.drawSelected = ui["selected"] ?? false
                return Step(state: ctx, query: try #require(s["query"] as? String),
                            hits: Set(try #require(s["hits"] as? [String])), nohit: Set(try #require(s["nohit"] as? [String])),
                            empty: try #require(s["empty"] as? Bool), searching: try #require(s["searching"] as? Bool))
            }
            return (try #require(c["id"] as? String), steps)
        }
    }

    @Test func theResultMatchesThePageForEveryRecordedQueryAndState() throws {
        let cases = try Self.corpus()
        #expect(cases.count > 1000)
        var shownBlocks = 0
        for c in cases {
            var engine = SearchEngine()
            for (i, step) in c.steps.enumerated() {
                let r = engine.run(query: step.query, step.state)
                let id = "\(c.id) step \(i)"
                #expect((r != nil) == step.searching, "\(id): searching")
                guard let r else { continue }
                #expect(r.show == step.hits, "\(id): rows")
                // the page hides the blocks that hold no row on show; ours lists the ones that do
                let hidden = Set(SearchBlock.allCases.map(\.rawValue)).subtracting(r.blocks.map(\.rawValue))
                #expect(hidden == step.nohit, "\(id): sections")
                #expect(r.none == step.empty, "\(id): No settings match")
                shownBlocks += r.blocks.count
            }
        }
        #expect(shownBlocks > 500)
    }
}
