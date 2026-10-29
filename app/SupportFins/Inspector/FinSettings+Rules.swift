import Foundation

/// The inspector's conditional visibility, the "N changed" count, the collapsed summaries and the input
/// limits: the web's "no control the math ignores" rules (`web/ui/settings.js`, `web/ui/more.js`), as pure
/// functions of `FinSettings` so they can be tested without a view.
extension FinSettings {
    /// Every mode but Draw places supports itself (`autoLike`, `web/ui/settings.js:42`).
    var autoLike: Bool { mode != .draw }

    /// Whether the tine direction is vertical *and* tines are on: resin pins (`web/ui/settings.js:178`).
    var pins: Bool { tines && tineDirection == .vertical }

    /// Which controls are shown or disabled for these settings.
    struct Visibility: Equatable {
        /// Layer height: tines on and not pins, or a pad / interface that reads it (`settings.js:186-188`).
        var layerHeight = true
        var tineGrip = true
        var tineDirection = true
        var pinDepth = false
        /// The resin note under the Tines section (`#resin-note`).
        var resinNote = false
        /// Hidden for Vertical tines (`settings.js:172`).
        var roundedFeet = true
        /// The pad's four numbers: only for Custom (`settings.js:117-153`).
        var padCustom = false
        var swayFrom = false
        var swaySpacing = false
        var swayDepth = false
        var fullBraceDepth = false
        var swayCrossHeight = false
        var tallRibsHeight = true
        /// Auto's slider: hidden in Draw, shown but disabled in Full coverage (`settings.js:46-62`).
        var coverage = true
        var coverageDisabled = false
        var longWalls = false
        var separateWalls = false
        var curvedWalls = true
    }

    var visibility: Visibility {
        var v = Visibility()
        let pinsOn = pins
        v.tineGrip = tines
        v.tineDirection = tines
        v.pinDepth = pinsOn
        v.resinNote = pinsOn
        v.roundedFeet = tineDirection != .vertical
        v.layerHeight = !((!tines || pinsOn) && ![.light, .auto].contains(bedPad) && interfaceMaterial == .off)
        v.padCustom = bedPad == .custom
        v.swayFrom = sway && tines
        v.swaySpacing = sway && tines
        v.swayDepth = sway
        v.fullBraceDepth = sway
        v.swayCrossHeight = sway && tallRibs
        v.tallRibsHeight = tallRibs
        v.coverage = autoLike
        v.coverageDisabled = mode == .full
        v.longWalls = mode == .full
        // only the fill pass joins walls (#309): Full's, and Auto's under Curved walls
        v.separateWalls = mode == .full || (autoLike && curvedWalls)
        v.curvedWalls = autoLike
        return v
    }

    /// The Placement note under the segmented control (`MODE_NOTES`, `settings.js:79-83`).
    var modeNote: String {
        switch mode {
        case .auto: JS.t("Places supports for you.")
        case .full: JS.t("Auto, plus walls under every overhang it leaves bare. More plastic.")
        case .draw: JS.t("You place each wall: click two points across an overhang.")
        }
    }

    // MARK: More settings: the changed count

    /// A field of the More settings section (`web/ui/more.js`).
    enum MoreField: CaseIterable, Hashable {
        case roundedFeet, bedPad, padThickness, padGap, padGrip, padSpread, wallLines
        case tallRibs, tallRibsHeight, sway, swayFrom, swaySpacing, swayDepth, fullBraceDepth, swayCrossHeight
        case coverage, plateOnly, separateWalls, longWalls, curvedWalls
        case highlightSmall, showIslands
    }

    /// The More settings fields whose value differs from the default **and** that the build reads right now:
    /// a hidden field (the pad numbers outside Custom, the sway fields with sway off, coverage in Draw) or a
    /// disabled one (coverage in Full coverage) does not count, and neither does a menu whose default option
    /// is disabled (`more.js:20-31`; the Wall lines counts a big nozzle cannot take, `settings.js:333-349`).
    var changedFields: Set<MoreField> {
        let d = FinSettings(), v = visibility
        var out = Set<MoreField>()
        func check(_ f: MoreField, _ differs: Bool, shown: Bool = true) { if differs && shown { out.insert(f) } }
        check(.roundedFeet, roundedFeet != d.roundedFeet, shown: v.roundedFeet)
        check(.bedPad, bedPad != d.bedPad)
        check(.padThickness, padThickness != d.padThickness, shown: v.padCustom)
        check(.padGap, padGap != d.padGap, shown: v.padCustom)
        check(.padGrip, padGrip != d.padGrip, shown: v.padCustom)
        check(.padSpread, padSpread != d.padSpread, shown: v.padCustom)
        check(.wallLines, wallLines != d.wallLines, shown: Self.wallLinesFit(d.wallLines, nozzle: nozzle))
        check(.tallRibs, tallRibs != d.tallRibs)
        check(.tallRibsHeight, tallRibsHeight != d.tallRibsHeight, shown: v.tallRibsHeight)
        check(.sway, sway != d.sway)
        check(.swayFrom, swayFrom != d.swayFrom, shown: v.swayFrom)
        check(.swaySpacing, swaySpacing != d.swaySpacing, shown: v.swaySpacing)
        check(.swayDepth, swayDepth != d.swayDepth, shown: v.swayDepth)
        check(.fullBraceDepth, fullBraceDepth != d.fullBraceDepth, shown: v.fullBraceDepth)
        check(.swayCrossHeight, swayCrossHeight != d.swayCrossHeight, shown: v.swayCrossHeight)
        check(.coverage, coverage != d.coverage, shown: v.coverage && !v.coverageDisabled)
        check(.plateOnly, plateOnly != d.plateOnly)
        check(.separateWalls, separateWalls != d.separateWalls, shown: v.separateWalls)
        check(.longWalls, longWalls != d.longWalls, shown: v.longWalls)
        check(.curvedWalls, curvedWalls != d.curvedWalls, shown: v.curvedWalls)
        check(.highlightSmall, highlightSmall != d.highlightSmall)
        check(.showIslands, showIslands != d.showIslands)
        return out
    }

    /// `"2 changed"`, or empty when nothing is (`more.js:49-50`).
    var changedSummary: String {
        let n = changedFields.count
        return n == 0 ? "" : JS.t("{n} changed", ["n": String(n)])
    }

    // MARK: collapsed summaries

    /// The Printer row (`syncSectionSums`, `settings.js:398-405`): `"0.4 nozzle · 0.2 layers · 0.2 gap"`;
    /// layer height drops out while its field is hidden, and `interface` appears when one is chosen.
    var printerSummary: String {
        [
            "\(Self.nozzleText(nozzle)) \(JS.t("nozzle"))",
            visibility.layerHeight ? "\(Self.numberText(layerHeight)) \(JS.t("layers"))" : nil,
            "\(Self.numberText(gap)) \(JS.t("gap"))",
            interfaceMaterial == .off ? nil : JS.t("interface"),
        ].compactMap { $0 }.joined(separator: " · ")
    }

    /// The Tines section's collapsed line (`settings.js:406-410`).
    var tinesSummary: String {
        if !tines { return "off" }
        let grip = tineDensity
        if tineDirection == .vertical {
            return "\(JS.t(grip <= 20 ? "fewer pins" : grip >= 80 ? "more pins" : "pins")) · \(JS.t("resin"))"
        }
        return grip <= 20 ? JS.t("light grip") : grip >= 80 ? JS.t("firm grip") : JS.t("medium grip")
    }

    // MARK: choices, limits and the rules that move a value

    /// The Nozzle menu (`index.html:287`), as the option values the web shows.
    static let nozzleChoices: [Double] = [0.2, 0.25, 0.4, 0.6, 0.8, 1.0, 1.2, 1.4, 1.6, 1.8, 2.0]
    /// The Wall lines menu (`WALL_LINES`, `web/nozzle.js:35`).
    static let wallLineChoices = [2, 4, 6, 8]

    /// A nozzle as its menu option's text (`"1.0"`, `"0.25"`): the web shows `el('nozzle').value`.
    static func nozzleText(_ d: Double) -> String {
        let t = numberText(d)
        return t.contains(".") ? t : t + ".0"
    }

    /// A number as JS prints it (`0.2`, `4`, `0.25`): the shortest text that reads back the same.
    static func numberText(_ v: Double) -> String {
        if v == v.rounded(), abs(v) < 1e15 { return String(Int64(v)) }
        return "\(v)"
    }

    /// Does an `n`-line wall fit under `TH_MAX` at this nozzle? The page offers only these
    /// (`wallLinesFit`, `web/nozzle.js:61`): `PROP.th` (1.0) x max(1, d / 0.4) x n / 2 <= 6.
    static func wallLinesFit(_ n: Int, nozzle d: Double) -> Bool {
        1.0 * max(1, d / 0.4) * Double(n) / 2 <= 6 + 1e-9
    }

    /// A count the nozzle cannot take drops to the widest that fits (`fitWallLines`, `settings.js:333-340`).
    mutating func fitWallLines() {
        if !Self.wallLinesFit(wallLines, nozzle: nozzle), let widest = Self.wallLineChoices.last(where: { Self.wallLinesFit($0, nozzle: nozzle) }) {
            wallLines = widest
        }
    }

    /// Choosing a nozzle (`settings.js:297-303`).
    mutating func setNozzle(_ d: Double) {
        nozzle = d
        fitWallLines()
    }

    /// Choosing a tine direction in the inspector sets Separate walls to match (resin cures joined walls
    /// into one block, #309); the box stays the user's to change after (`settings.js:171-178`).
    mutating func chooseTineDirection(_ d: TineDirection) {
        tineDirection = d
        separateWalls = d == .vertical
    }

    /// The input limits of the numeric fields (`index.html`, docs/UI-MAP.md 4.1-4.2): `min...max`, `step`.
    struct Limits { let range: ClosedRange<Double>; let step: Double }
    static let layerHeightLimits = Limits(range: 0.08...0.6, step: 0.04)
    static let gapLimits = Limits(range: 0.1...0.4, step: 0.05)
    static let pinDepthLimits = Limits(range: 0.05...1, step: 0.05)
    static let padThicknessLimits = Limits(range: 0.1...1.0, step: 0.05)
    static let padGapLimits = Limits(range: 0...0.5, step: 0.01)
    static let padGripLimits = Limits(range: -0.2...0.3, step: 0.05)
    static let padSpreadLimits = Limits(range: 1...12, step: 0.5)
    static let tallRibsHeightLimits = Limits(range: 5...200, step: 0.1)
    static let swayFromLimits = Limits(range: 0...2000, step: 5)
    static let swaySpacingLimits = Limits(range: 2...30, step: 1)
    static let swayDepthLimits = Limits(range: 5...50, step: 1)
    static let swayCrossHeightLimits = Limits(range: 10...90, step: 1)
}
