import Foundation

/// What the search reads of the window to decide which rows exist: the settings (their `visibility` is the web's
/// `hidden` on each field) and the state of the features whose controls sit in the inspector (Draw, Remove fins).
struct SearchContext: Equatable, Sendable {
    var settings = FinSettings()
    /// "+ Add walls by hand" is offered (`DrawModel.augmentOffered`).
    var augmentOffered = false
    /// Remove fins / Restore all are offered, and there is something to restore (`RemovalModel`).
    var removeControls = false
    var canRestore = false
    /// `#draw-controls` is shown; Line / Curve while a click places a wall; Back one point while one is half drawn;
    /// Remove selected while one is selected.
    var drawControls = false
    var drawHint = false
    var drawStarted = false
    var drawSelected = false
}

/// The inspector's folded blocks, which a search opens when they hold a match (`details.sec`) or hides when they hold none.
enum SearchBlock: String, CaseIterable {
    case printer, placement, tines, more, draw
    /// The four groups inside More settings (`.group`).
    case holdThePart, tallParts, whereSupportsGo, display
}

/// One setting row of the options panel (`ROWS` in `web/ui/search.js`: a field, a checkbox, Placement or an action
/// button), as the page has it. `names` is the row's text as the page shows it, piece by piece (labels, units, the
/// options of its menu), in English: the search matches the localized pieces and these. `tip` is the row's tooltip.
/// `shown` is the row not being `hidden` for this window; `gate` the id of the row that reveals it (`GATES`).
struct SearchEntry: Sendable {
    let id: String
    let blocks: [SearchBlock]
    let names: @Sendable (SearchContext) -> [String]
    /// The tooltip: what the row's `title` is now (localized for the search) and the English `origTitle` the page keeps beside it.
    /// They differ for the two rows that swap their title (Tine grip for pins, Wide-face coverage in Full), whose `origTitle`
    /// stays the first title (`settings.js` `gripTip`, `coverTip`).
    let tip: @Sendable (SearchContext) -> (shown: String, orig: String)
    let shown: @Sendable (SearchContext) -> Bool
    let gate: @Sendable (SearchContext) -> String?

    init(_ id: String, _ blocks: SearchBlock..., names: @escaping @Sendable (SearchContext) -> [String], tip: @escaping @Sendable (SearchContext) -> (shown: String, orig: String) = { _ in ("", "") },
         shown: @escaping @Sendable (SearchContext) -> Bool = { _ in true }, gate: @escaping @Sendable (SearchContext) -> String? = { _ in nil }) {
        self.id = id
        self.blocks = blocks
        self.names = names
        self.tip = tip
        self.shown = shown
        self.gate = gate
    }

    init(_ id: String, _ blocks: SearchBlock..., names: [String], tip: String = "",
         shown: @escaping @Sendable (SearchContext) -> Bool = { _ in true }, gate: @escaping @Sendable (SearchContext) -> String? = { _ in nil }) {
        self.init(id, blocks: blocks, names: { _ in names }, tip: { _ in (tip, tip) }, shown: shown, gate: gate)
    }

    private init(_ id: String, blocks: [SearchBlock], names: @escaping @Sendable (SearchContext) -> [String], tip: @escaping @Sendable (SearchContext) -> (shown: String, orig: String),
                 shown: @escaping @Sendable (SearchContext) -> Bool, gate: @escaping @Sendable (SearchContext) -> String?) {
        self.id = id
        self.blocks = blocks
        self.names = names
        self.tip = tip
        self.shown = shown
        self.gate = gate
    }
}

/// The rows of the inspector in the page's order (`index.html:255-593`), with the rules of `GATES`.
enum SearchCatalog {
    static let rows: [SearchEntry] = [
        // Printer
        SearchEntry("nozzle-fld", .printer, names: ["Nozzle", "mm", "0.2", "0.25", "0.4", "0.6", "0.8", "1.0", "1.2", "1.4", "1.6", "1.8", "2.0"], tip: Tip.nozzle),
        SearchEntry("layerh-fld", .printer, names: ["Layer height", "mm"], tip: Tip.layerHeight,
                    shown: { $0.settings.visibility.layerHeight }, gate: { _ in "tines" }),
        SearchEntry("gap", .printer, names: ["Support gap", "mm"], tip: Tip.gap),
        SearchEntry("iface", .printer, names: ["Interface material", "Off", "Flat contacts", "Everywhere"], tip: Tip.iface),
        // Placement: its hidden menu and its note under the buttons are not part of its name
        SearchEntry("fin-mode-fld", .placement, names: ["Placement", "Auto", "Full coverage", "Draw"]),
        // Tines
        SearchEntry("tines", .tines, names: ["Tines", "grip the part"], tip: Tip.tines),
        SearchEntry("tinegrip-fld", .tines,
                    names: { $0.settings.pins ? ["Pins per wall", "fewer", "⟶", "more"] : ["Tine grip", "light", "⟶", "firm"] },
                    tip: { $0.settings.pins ? (TinesSection.pinSpacingTip, Tip.tineDensity) : (Tip.tineDensity, Tip.tineDensity) },
                    shown: { $0.settings.visibility.tineGrip }, gate: { _ in "tines" }),
        SearchEntry("tinedir-fld", .tines, names: ["Direction", "Horizontal (FDM)", "Vertical (resin)"], tip: Tip.tineDir,
                    shown: { $0.settings.visibility.tineDirection }),
        SearchEntry("pindepth-fld", .tines, names: ["Pin depth"], tip: Tip.pinDepth, shown: { $0.settings.visibility.pinDepth }),
        // Basics
        SearchEntry("cutout", names: ["Cutouts", "None", "Diamond", "Triangle", "Arch", "Lattice", "Honeycomb"], tip: Tip.cutout),
        SearchEntry("augment-toggle", names: ["Add walls by hand"], tip: DrawTip.augment,
                    shown: { $0.augmentOffered }, gate: { _ in "fin-mode-fld" }),
        SearchEntry("remove-fins-toggle", names: ["Remove fins"], tip: "Click a fin to remove just that one. Esc or right-click cancels.",
                    shown: { $0.removeControls }, gate: { _ in "fin-mode-fld" }),
        SearchEntry("restore-fins", names: ["Restore all"], tip: "Bring back every fin removed in this orientation.",
                    shown: { $0.removeControls && $0.canRestore }, gate: { _ in "remove-fins-toggle" }),
        // Draw controls
        // the page's Line and Curve buttons are one segmented control here
        SearchEntry("draw-shape", .draw, names: ["Line", "Curve"], shown: { $0.drawControls && $0.drawHint }),
        SearchEntry("draw-back", .draw, names: ["Back one point"], tip: DrawTip.back, shown: { $0.drawControls && $0.drawStarted }),
        SearchEntry("draw-remove", .draw, names: ["Remove selected"], tip: DrawTip.removeSelected, shown: { $0.drawControls && $0.drawSelected }),
        SearchEntry("draw-clear", .draw, names: ["Clear all"], shown: { $0.drawControls },
                    gate: { $0.settings.mode == .draw ? nil : "fin-mode-fld" }),
        // More settings > Hold the part
        SearchEntry("rounded-feet-fld", .more, .holdThePart, names: ["Rounded feet"], tip: Tip.roundedFeet, shown: { $0.settings.visibility.roundedFeet }),
        SearchEntry("bed-pad", .more, .holdThePart, names: ["Bed pad", "Off", "Auto", "Light", "Sure hold", "Custom"], tip: Tip.bedPad),
        SearchEntry("pad-h", .more, .holdThePart, names: ["Pad thickness", "mm"], tip: Tip.padH,
                    shown: { $0.settings.visibility.padCustom }, gate: { _ in "bed-pad" }),
        SearchEntry("pad-gap", .more, .holdThePart, names: ["Pad gap", "mm"], tip: Tip.padGap,
                    shown: { $0.settings.visibility.padCustom }, gate: { _ in "bed-pad" }),
        SearchEntry("pad-grip", .more, .holdThePart, names: ["Pad grip", "mm"], tip: Tip.padGrip,
                    shown: { $0.settings.visibility.padCustom }, gate: { _ in "bed-pad" }),
        SearchEntry("pad-margin", .more, .holdThePart, names: ["Pad spread", "mm"], tip: Tip.padMargin,
                    shown: { $0.settings.visibility.padCustom }, gate: { _ in "bed-pad" }),
        SearchEntry("wall-lines", .more, .holdThePart, names: ["Wall lines", "2", "4", "6", "8"], tip: Tip.wallLines),
        // Tall parts
        SearchEntry("tall-ribs", .more, .tallParts, names: ["Brace tall walls"], tip: Tip.tallRibs),
        SearchEntry("tall-ribs-height-fld", .more, .tallParts, names: ["Brace walls above", "cm"], tip: Tip.tallRibsHeight,
                    shown: { $0.settings.visibility.tallRibsHeight }, gate: { _ in "tall-ribs" }),
        SearchEntry("sway", .more, .tallParts, names: ["Sway braces"], tip: Tip.sway),
        // the brace tine fields also hide with Tines off (settings.js syncSway)
        SearchEntry("sway-from-fld", .more, .tallParts, names: ["Brace grip from", "mm up"], tip: Tip.swayFrom,
                    shown: { $0.settings.visibility.swayFrom }, gate: { $0.settings.sway ? "tines" : "sway" }),
        SearchEntry("sway-spacing-fld", .more, .tallParts, names: ["Brace tine spacing", "mm"], tip: Tip.swaySpacing,
                    shown: { $0.settings.visibility.swaySpacing }, gate: { $0.settings.sway ? "tines" : "sway" }),
        SearchEntry("sway-depth-fld", .more, .tallParts, names: ["Brace depth", "% of height"], tip: Tip.swayDepth,
                    shown: { $0.settings.visibility.swayDepth }, gate: { _ in "sway" }),
        SearchEntry("full-brace-depth-fld", .more, .tallParts, names: ["Full brace depth"], tip: Tip.fullBraceDepth,
                    shown: { $0.settings.visibility.fullBraceDepth }, gate: { _ in "sway" }),
        SearchEntry("sway-cross-height-fld", .more, .tallParts, names: ["Cross-rib height", "% of height"], tip: Tip.swayCrossHeight,
                    shown: { $0.settings.visibility.swayCrossHeight }, gate: { $0.settings.sway ? "tall-ribs" : "sway" }),
        // Where supports go
        SearchEntry("coverage-fld", .more, .whereSupportsGo, names: { _ in ["Wide-face coverage", "sparse", "⟶", "dense"] },
                    tip: { $0.settings.visibility.coverageDisabled ? (MoreSettingsSection.coverageTipFull, Tip.coverage) : (Tip.coverage, Tip.coverage) },
                    shown: { $0.settings.visibility.coverage }, gate: { _ in "fin-mode-fld" }),
        SearchEntry("plate-only", .more, .whereSupportsGo, names: ["Plate only"], tip: Tip.plateOnly),
        SearchEntry("separate-walls-fld", .more, .whereSupportsGo, names: ["Separate walls"], tip: Tip.separateWalls,
                    shown: { $0.settings.visibility.separateWalls }),
        SearchEntry("long-walls-fld", .more, .whereSupportsGo, names: ["Long walls", "along the part"], tip: Tip.longWalls,
                    shown: { $0.settings.visibility.longWalls }),
        SearchEntry("curved-walls-fld", .more, .whereSupportsGo, names: ["Curved walls", "follow the curve, more plastic"], tip: Tip.curvedWalls,
                    shown: { $0.settings.visibility.curvedWalls }),
        // Display
        SearchEntry("highlight-small", .more, .display, names: ["Highlight small overhangs"], tip: Tip.highlightSmall),
        SearchEntry("show-islands", .more, .display, names: ["Show midair starts"], tip: Tip.showIslands),
    ]

    static let byID: [String: SearchEntry] = Dictionary(uniqueKeysWithValues: rows.map { ($0.id, $0) })
}
