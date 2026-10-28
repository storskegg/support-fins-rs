import Foundation
import Testing
@testable import Support_Fins

/// The inspector's rules: which controls show, what counts as changed, the collapsed summaries, the wall-line
/// limits, the remembered values and the web's tooltips. Each expectation cites the web line it comes from.
@Suite("Inspector rules")
struct InspectorRulesTests {
    private static var root: URL {
        URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    }

    // MARK: visibility (web/ui/settings.js)

    @Test func theDefaultsShowWhatTheWebShowsAtLoad() {
        let v = FinSettings().visibility
        #expect(v.layerHeight && v.tineGrip && v.tineDirection && v.roundedFeet && v.coverage && v.curvedWalls && v.tallRibsHeight)
        #expect(!v.pinDepth && !v.resinNote && !v.padCustom && !v.longWalls && !v.separateWalls && !v.coverageDisabled)
        #expect(!v.swayFrom && !v.swaySpacing && !v.swayDepth && !v.fullBraceDepth && !v.swayCrossHeight)
    }

    @Test func tinesOffHidesGripDirectionAndMaybeLayerHeight() {
        var s = FinSettings()
        s.tines = false
        var v = s.visibility
        #expect(!v.tineGrip && !v.tineDirection)
        #expect(v.layerHeight, "the Auto pad reads the layer height (settings.js:186)")
        s.bedPad = .off
        #expect(!s.visibility.layerHeight)
        s.interfaceMaterial = .flat
        #expect(s.visibility.layerHeight, "the interface band is a layer of the wall top")
        s.interfaceMaterial = .off
        s.bedPad = .light
        v = s.visibility
        #expect(v.layerHeight)
    }

    @Test func verticalTinesAreResinPins() {
        var s = FinSettings()
        s.tineDirection = .vertical
        var v = s.visibility
        #expect(v.pinDepth && v.resinNote && !v.roundedFeet)
        #expect(v.layerHeight, "the Auto pad still reads it")
        s.bedPad = .off
        #expect(!s.visibility.layerHeight, "pins are no layer tall (settings.js:184)")
        s.tines = false
        v = s.visibility
        #expect(!v.pinDepth && !v.resinNote)
        #expect(!v.roundedFeet, "hidden by the direction, tines or not (settings.js:172)")
    }

    @Test func padNumbersOnlyForCustomAndSwayFieldsFollowSway() {
        var s = FinSettings()
        for pad in FinSettings.BedPad.allCases { s.bedPad = pad; #expect(s.visibility.padCustom == (pad == .custom)) }
        s.sway = true
        var v = s.visibility
        #expect(v.swayFrom && v.swaySpacing && v.swayDepth && v.fullBraceDepth && v.swayCrossHeight)
        s.tines = false
        v = s.visibility
        #expect(!v.swayFrom && !v.swaySpacing && v.swayDepth, "the two tine settings follow Tines (settings.js:214-241)")
        s.tallRibs = false
        v = s.visibility
        #expect(!v.swayCrossHeight && !v.tallRibsHeight)
    }

    @Test func modeGatesTheCoverageAndWallJoiningControls() {
        var s = FinSettings()
        var v = s.visibility
        #expect(v.coverage && !v.coverageDisabled && !v.longWalls && !v.separateWalls && v.curvedWalls)
        s.curvedWalls = true
        #expect(s.visibility.separateWalls, "Separate walls shows with Curved walls in Auto (settings.js:62)")
        s.curvedWalls = false
        s.mode = .full
        v = s.visibility
        #expect(v.coverage && v.coverageDisabled && v.longWalls && v.separateWalls && v.curvedWalls)
        s.mode = .draw
        v = s.visibility
        #expect(!v.coverage && !v.longWalls && !v.separateWalls && !v.curvedWalls)
        #expect(!s.autoLike && FinSettings.Mode.allCases.filter { var t = FinSettings(); t.mode = $0; return t.autoLike }.count == 2)
    }

    @Test func eachModeHasItsNote() {
        var s = FinSettings()
        #expect(s.modeNote == "Places supports for you.")
        s.mode = .full
        #expect(s.modeNote == "Auto, plus walls under every overhang it leaves bare. More plastic.")
        s.mode = .draw
        #expect(s.modeNote == "You place each wall: click two points across an overhang.")
    }

    // MARK: the changed count (web/ui/more.js)

    @Test func nothingIsChangedAtTheDefaults() {
        #expect(FinSettings().changedFields.isEmpty)
        #expect(FinSettings().changedSummary == "")
    }

    @Test func aChangedFieldCountsAndShowsItsDot() {
        var s = FinSettings()
        s.plateOnly = true
        s.wallLines = 4
        #expect(s.changedFields == [.plateOnly, .wallLines])
        #expect(s.changedSummary == "2 changed")
        s.highlightSmall = false
        s.showIslands = true
        #expect(s.changedSummary == "4 changed")
    }

    @Test func aFieldTheBuildIgnoresDoesNotCount() {
        var s = FinSettings()
        // pad numbers outside Custom
        s.padGap = 0.3
        s.padSpread = 9
        #expect(s.changedFields.isEmpty)
        s.bedPad = .custom
        #expect(s.changedFields == [.bedPad, .padGap, .padSpread])
        // sway fields with sway off
        s = FinSettings()
        s.swayDepth = 30
        s.fullBraceDepth = true
        #expect(s.changedFields.isEmpty)
        s.sway = true
        #expect(s.changedFields == [.sway, .swayDepth, .fullBraceDepth])
        // coverage: disabled in Full coverage, hidden in Draw
        s = FinSettings()
        s.coverage = 80
        #expect(s.changedFields == [.coverage])
        s.mode = .full
        #expect(s.changedFields.isEmpty)
        s.mode = .draw
        #expect(s.changedFields.isEmpty)
        // Long walls is Full's; Separate walls needs Full or Curved walls
        s = FinSettings()
        s.longWalls = false
        s.separateWalls = true
        #expect(s.changedFields.isEmpty)
        s.mode = .full
        #expect(s.changedFields == [.longWalls, .separateWalls])
        // tall-rib height with the ribs off; rounded feet under Vertical tines
        s = FinSettings()
        s.tallRibs = false
        s.tallRibsHeight = 50
        #expect(s.changedFields == [.tallRibs])
        s = FinSettings()
        s.roundedFeet = true
        s.tineDirection = .vertical
        #expect(s.changedFields.isEmpty)
    }

    // MARK: summaries (web/ui/settings.js syncSectionSums)

    @Test func thePrinterLineNamesOnlyWhatTheBuildReads() {
        var s = FinSettings()
        #expect(s.printerSummary == "0.4 nozzle · 0.2 layers · 0.2 gap")
        s.setNozzle(1.0)
        s.layerHeight = 0.12
        s.gap = 0.25
        s.interfaceMaterial = .all
        #expect(s.printerSummary == "1.0 nozzle · 0.12 layers · 0.25 gap · interface")
        s.interfaceMaterial = .off
        s.tines = false
        s.bedPad = .off
        #expect(s.printerSummary == "1.0 nozzle · 0.25 gap")
    }

    @Test func theTinesLineFollowsTheGripAndDirection() {
        var s = FinSettings()
        for (grip, want) in [(0.0, "light grip"), (20, "light grip"), (25, "medium grip"), (75, "medium grip"), (80, "firm grip"), (100, "firm grip")] {
            s.tineDensity = grip
            #expect(s.tinesSummary == want, "\(grip)")
        }
        s.tineDirection = .vertical
        for (grip, want) in [(0.0, "fewer pins · resin"), (50, "pins · resin"), (100, "more pins · resin")] {
            s.tineDensity = grip
            #expect(s.tinesSummary == want, "\(grip)")
        }
        s.tines = false
        #expect(s.tinesSummary == "off")
    }

    @Test func numbersPrintAsTheWebPrintsThem() {
        #expect(FinSettings.nozzleText(0.4) == "0.4" && FinSettings.nozzleText(1) == "1.0" && FinSettings.nozzleText(0.25) == "0.25" && FinSettings.nozzleText(2) == "2.0")
        #expect(FinSettings.numberText(0.2) == "0.2" && FinSettings.numberText(4) == "4" && FinSettings.numberText(0.12) == "0.12")
    }

    // MARK: wall lines and the nozzle

    @Test func wallLinesFitMatchesTheWeb() throws {
        let text = try String(contentsOf: Self.root.appending(path: "parity/corpus/wall_lines/wallLinesFit.jsonl"), encoding: .utf8)
        var n = 0
        for line in text.split(separator: "\n") {
            let obj = try #require(try JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any])
            let args = try #require(obj["args"] as? [Double])
            let want = try #require(obj["out"] as? Bool)
            #expect(FinSettings.wallLinesFit(Int(args[0]), nozzle: args[1]) == want, "\(obj["id"] ?? "")")
            n += 1
        }
        #expect(n >= 50)
    }

    @Test func aBigNozzleDropsWallLinesToTheWidestThatFits() {
        var s = FinSettings()
        s.wallLines = 8
        s.setNozzle(1.0)
        #expect(s.wallLines == 4)
        s.setNozzle(2.0)
        #expect(s.wallLines == 2)
        s.setNozzle(0.4)
        #expect(s.wallLines == 2, "a smaller nozzle never raises it")
        s.wallLines = 6
        s.setNozzle(0.6)
        #expect(s.wallLines == 6)
        #expect(FinSettings.wallLineChoices.filter { FinSettings.wallLinesFit($0, nozzle: 2.0) } == [2])
    }

    @Test func choosingVerticalTinesTicksSeparateWalls() {
        var s = FinSettings()
        s.chooseTineDirection(.vertical)
        #expect(s.tineDirection == .vertical && s.separateWalls)
        s.separateWalls = false // the box stays the user's
        #expect(!s.separateWalls)
        s.chooseTineDirection(.horizontal)
        #expect(s.tineDirection == .horizontal && !s.separateWalls)
    }

    // MARK: remembered values (docs/UI-MAP.md section 9)

    private func suite() -> UserDefaults {
        let name = "test.\(UUID().uuidString)"
        let d = UserDefaults(suiteName: name)!
        d.removePersistentDomain(forName: name)
        return d
    }

    @Test func theRememberedValuesRoundTrip() {
        let store = SettingsStore(defaults: suite())
        #expect(store.load() == FinSettings(), "nothing stored: the defaults")
        var s = FinSettings()
        s.setNozzle(0.6)
        s.layerHeight = 0.12
        s.gap = 0.3
        s.highlightSmall = false
        s.showIslands = true
        s.plateOnly = true // not remembered
        store.save(s)
        var want = FinSettings()
        want.setNozzle(0.6)
        want.layerHeight = 0.12
        want.gap = 0.3
        want.highlightSmall = false
        want.showIslands = true
        #expect(store.load() == want)
        #expect(store.defaults.object(forKey: "sf.nozzle") as? Double == 0.6)
        #expect(store.defaults.object(forKey: "sf.layerHeight") as? Double == 0.12)
        #expect(store.defaults.object(forKey: "sf.gap") as? Double == 0.3)
    }

    @Test func aStoredValueOutOfRangeIsIgnored() {
        let d = suite()
        d.set(0.33, forKey: "sf.nozzle") // not on the menu
        d.set(5.0, forKey: "sf.layerHeight") // over 0.6
        d.set(0.05, forKey: "sf.gap") // under 0.1
        #expect(SettingsStore(defaults: d).load() == FinSettings())
    }

    @Test func aRememberedBigNozzleStillFitsTheWallLines() {
        let d = suite()
        d.set(2.0, forKey: "sf.nozzle")
        var base = FinSettings()
        base.wallLines = 6
        #expect(SettingsStore(defaults: d).load(into: base).wallLines == 2)
    }

    @Test func sectionStateRoundTripsAndStartsWithTinesOpen() {
        let d = SectionState()
        #expect(d.tines && !d.printer && !d.more)
        #expect(SectionState(rawValue: "") == d)
        var s = d
        s.more = true
        s.tines = false
        #expect(SectionState(rawValue: s.rawValue) == s)
        #expect(SectionState(rawValue: "not json") == nil)
    }

    // MARK: tooltips, verbatim

    @Test func everyTooltipIsTheWebsTextFromTheUIMap() throws {
        let text = try String(contentsOf: Self.root.appending(path: "docs/UI-MAP.md"), encoding: .utf8)
        var tips: [String: String] = [:]
        for line in text.split(separator: "\n") where line.hasPrefix("| `") {
            let c = line.split(separator: "|", omittingEmptySubsequences: false).dropFirst().map { $0.trimmingCharacters(in: .whitespaces) }
            if c.count >= 9 { tips[c[0].trimmingCharacters(in: CharacterSet(charactersIn: "`"))] = c[5] }
        }
        let table: [(String, String)] = [
            ("nozzle", Tip.nozzle), ("layer-height", Tip.layerHeight), ("gap", Tip.gap), ("iface", Tip.iface),
            ("fin-mode", Tip.finMode), ("tines", Tip.tines), ("tine-density", Tip.tineDensity), ("tine-dir", Tip.tineDir),
            ("pin-depth", Tip.pinDepth), ("cutout", Tip.cutout), ("rounded-feet", Tip.roundedFeet), ("bed-pad", Tip.bedPad),
            ("pad-h", Tip.padH), ("pad-gap", Tip.padGap), ("pad-grip", Tip.padGrip), ("pad-margin", Tip.padMargin),
            ("wall-lines", Tip.wallLines), ("tall-ribs", Tip.tallRibs), ("tall-ribs-height", Tip.tallRibsHeight),
            ("sway", Tip.sway), ("sway-from", Tip.swayFrom), ("sway-spacing", Tip.swaySpacing), ("sway-depth", Tip.swayDepth),
            ("full-brace-depth", Tip.fullBraceDepth), ("sway-cross-height", Tip.swayCrossHeight), ("coverage", Tip.coverage),
            ("plate-only", Tip.plateOnly), ("separate-walls", Tip.separateWalls), ("long-walls", Tip.longWalls),
            ("curved-walls", Tip.curvedWalls), ("highlight-small", Tip.highlightSmall), ("show-islands", Tip.showIslands),
            ("r-info", Tip.rInfo),
        ]
        #expect(table.count == 33)
        for (id, tip) in table { #expect(tips[id] == tip, "\(id)") }
    }
}
