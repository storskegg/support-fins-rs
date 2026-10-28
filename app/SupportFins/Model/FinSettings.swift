import Foundation

/// Every setting of the inspector (docs/UI-MAP.md 4.1-4.2) at its web default, in the web's units (a percent
/// slider holds 0...100, tall-rib height is in cm). A value type: the session snapshots it for undo and the
/// build model turns it into the engine's options with `optionsJSON()`.
///
/// Two JSON texts carry them to the engine (`docs/FFI.md`, "M4b as implemented"): `optionsJSON()` is the
/// `options.json` keys plus four request extras, and `tunablesJSON()` is the web's `finOpts().tunables`
/// (`web/ui/finbuild.js:139-147`), which the web sends to its Worker.
struct FinSettings: Codable, Equatable, Sendable {
    enum Mode: String, Codable, Sendable, CaseIterable { case auto, full, draw }
    enum TineDirection: String, Codable, Sendable, CaseIterable { case horizontal, vertical }
    enum Cutout: String, Codable, Sendable, CaseIterable { case none, diamond, triangle, arch, lattice, honeycomb }
    enum BedPad: String, Codable, Sendable, CaseIterable { case off, auto, light, sure, custom }
    enum InterfaceMaterial: String, Codable, Sendable, CaseIterable { case off, flat, all }

    // Toolbar: Overhang (`thr`), part of the engine request.
    var threshold = 45.0

    // Printer
    var nozzle = 0.4
    var layerHeight = 0.2
    var gap = 0.2
    var interfaceMaterial = InterfaceMaterial.off

    // Placement
    var mode = Mode.auto

    // Tines
    var tines = true
    /// 0...100 (the slider); the engine takes it / 100. Labelled "Pins per wall" for Vertical.
    var tineDensity = 0.0
    var tineDirection = TineDirection.horizontal
    var pinDepth = 0.1

    // Basics
    var cutout = Cutout.none

    // More settings > Hold the part
    var roundedFeet = false
    var bedPad = BedPad.auto
    var padThickness = 0.2
    var padGap = 0.12
    var padGrip = 0.0
    var padSpread = 4.0
    var wallLines = 2

    // More settings > Tall parts
    var tallRibs = true
    /// cm
    var tallRibsHeight = 10.0
    var sway = false
    var swayFrom = 0.0
    var swaySpacing = 6.0
    /// % of height; the engine takes it / 100.
    var swayDepth = 15.0
    var fullBraceDepth = false
    /// % of height
    var swayCrossHeight = 50.0

    // More settings > Where supports go
    /// 0...100; the engine takes it / 100.
    var coverage = 50.0
    var plateOnly = false
    /// Ticks itself for Vertical (resin) tines in the inspector; the engine takes the box as it stands.
    var separateWalls = false
    var longWalls = true
    var curvedWalls = false

    // More settings > Display (not part of a build)
    var highlightSmall = true
    var showIslands = false

    init() {}

    /// Missing keys take the web default, so a stored or recorded partial settings object decodes.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        func get<T: Decodable>(_ key: Key, _ keep: inout T) throws {
            if let v = try c.decodeIfPresent(T.self, forKey: key) { keep = v }
        }
        try get(.threshold, &threshold)
        try get(.nozzle, &nozzle)
        try get(.layerHeight, &layerHeight)
        try get(.gap, &gap)
        try get(.interfaceMaterial, &interfaceMaterial)
        try get(.mode, &mode)
        try get(.tines, &tines)
        try get(.tineDensity, &tineDensity)
        try get(.tineDirection, &tineDirection)
        try get(.pinDepth, &pinDepth)
        try get(.cutout, &cutout)
        try get(.roundedFeet, &roundedFeet)
        try get(.bedPad, &bedPad)
        try get(.padThickness, &padThickness)
        try get(.padGap, &padGap)
        try get(.padGrip, &padGrip)
        try get(.padSpread, &padSpread)
        try get(.wallLines, &wallLines)
        try get(.tallRibs, &tallRibs)
        try get(.tallRibsHeight, &tallRibsHeight)
        try get(.sway, &sway)
        try get(.swayFrom, &swayFrom)
        try get(.swaySpacing, &swaySpacing)
        try get(.swayDepth, &swayDepth)
        try get(.fullBraceDepth, &fullBraceDepth)
        try get(.swayCrossHeight, &swayCrossHeight)
        try get(.coverage, &coverage)
        try get(.plateOnly, &plateOnly)
        try get(.separateWalls, &separateWalls)
        try get(.longWalls, &longWalls)
        try get(.curvedWalls, &curvedWalls)
        try get(.highlightSmall, &highlightSmall)
        try get(.showIslands, &showIslands)
    }

    private enum Key: String, CodingKey {
        case threshold, nozzle, layerHeight, gap, interfaceMaterial, mode, tines, tineDensity, tineDirection, pinDepth,
             cutout, roundedFeet, bedPad, padThickness, padGap, padGrip, padSpread, wallLines, tallRibs, tallRibsHeight,
             sway, swayFrom, swaySpacing, swayDepth, fullBraceDepth, swayCrossHeight, coverage, plateOnly,
             separateWalls, longWalls, curvedWalls, highlightSmall, showIslands
    }

    /// Encoded in full, so a stored settings value is complete.
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(threshold, forKey: .threshold)
        try c.encode(nozzle, forKey: .nozzle)
        try c.encode(layerHeight, forKey: .layerHeight)
        try c.encode(gap, forKey: .gap)
        try c.encode(interfaceMaterial, forKey: .interfaceMaterial)
        try c.encode(mode, forKey: .mode)
        try c.encode(tines, forKey: .tines)
        try c.encode(tineDensity, forKey: .tineDensity)
        try c.encode(tineDirection, forKey: .tineDirection)
        try c.encode(pinDepth, forKey: .pinDepth)
        try c.encode(cutout, forKey: .cutout)
        try c.encode(roundedFeet, forKey: .roundedFeet)
        try c.encode(bedPad, forKey: .bedPad)
        try c.encode(padThickness, forKey: .padThickness)
        try c.encode(padGap, forKey: .padGap)
        try c.encode(padGrip, forKey: .padGrip)
        try c.encode(padSpread, forKey: .padSpread)
        try c.encode(wallLines, forKey: .wallLines)
        try c.encode(tallRibs, forKey: .tallRibs)
        try c.encode(tallRibsHeight, forKey: .tallRibsHeight)
        try c.encode(sway, forKey: .sway)
        try c.encode(swayFrom, forKey: .swayFrom)
        try c.encode(swaySpacing, forKey: .swaySpacing)
        try c.encode(swayDepth, forKey: .swayDepth)
        try c.encode(fullBraceDepth, forKey: .fullBraceDepth)
        try c.encode(swayCrossHeight, forKey: .swayCrossHeight)
        try c.encode(coverage, forKey: .coverage)
        try c.encode(plateOnly, forKey: .plateOnly)
        try c.encode(separateWalls, forKey: .separateWalls)
        try c.encode(longWalls, forKey: .longWalls)
        try c.encode(curvedWalls, forKey: .curvedWalls)
        try c.encode(highlightSmall, forKey: .highlightSmall)
        try c.encode(showIslands, forKey: .showIslands)
    }

    // MARK: the engine's options

    /// The `opts_json` of `Builder.build` / `build_part`: a JSON object of `options.json` keys in the entry's
    /// units, which the engine runs through `optionsFromDialog`, exactly what the web's `finOpts` /
    /// `swayOpts` (`web/ui/finbuild.js:115-160`) ask for. Every key is written, so the request does not depend
    /// on the engine's defaults.
    ///
    /// - Draw (M6) places walls by hand, so it sends no `mode` and no sway braces.
    /// - Full coverage sends `coverage` 0.5 whatever the slider says (`FIN.coverDefault`): the slider is Auto's.
    /// - Sway braces send only `sway.on` when off; the other sway keys only when on.
    /// - Custom pad numbers have no key: `padStyle` is left out; they travel in `tunablesJSON()` (`padCustom`).
    /// - Four request extras ride here beside the dialog keys (FFI.md): `tallRibs`, `curvedWalls` (Auto and Full
    ///   only, like `autoLike()`) and, with sway on, `sway.uncappedDepth` and `sway.crossRise`.
    func optionsJSON() -> String {
        var o: [(String, String)] = []
        func put(_ key: String, _ value: Double) { o.append((key, Self.number(value))) }
        func put(_ key: String, _ value: Bool) { o.append((key, value ? "true" : "false")) }
        func put(_ key: String, _ value: String) { o.append((key, "\"\(value)\"")) }

        if mode != .draw { put("mode", mode.rawValue) }
        put("threshold", threshold)
        put("nozzle", nozzle)
        put("tines", tines)
        put("tineDensity", tineDensity / 100)
        put("tineDir", tineDirection.rawValue)
        put("pinDepth", pinDepth)
        put("layerHeight", layerHeight)
        put("gap", gap)
        if bedPad != .custom { put("padStyle", bedPad.rawValue) }
        put("cutout", cutout.rawValue)
        put("coverage", mode == .full ? 0.5 : coverage / 100)
        put("plateOnly", plateOnly)
        put("separateWalls", separateWalls ? "on" : "off")
        put("longWalls", longWalls)
        put("tallRibs", tallRibs)
        put("curvedWalls", curvedWalls && mode != .draw)
        let braces = sway && mode != .draw
        put("sway.on", braces)
        if braces {
            put("sway.gripFrom", swayFrom)
            put("sway.tineSpacing", swaySpacing)
            put("sway.reach", swayDepth / 100)
            put("sway.uncappedDepth", fullBraceDepth)
            put("sway.crossRise", min(90, max(10, swayCrossHeight)) / 100)
        }
        return "{" + o.map { "\"\($0.0)\":\($0.1)" }.joined(separator: ",") + "}"
    }

    /// A finite double as JSON (`1.0` as `1`), the shortest text that reads back as the same value.
    private static func number(_ v: Double) -> String {
        precondition(v.isFinite, "a setting is not a finite number")
        if v == v.rounded(), abs(v) < 1e15 { return String(Int64(v)) }
        return "\(v)"
    }

    // MARK: the page's tunables

    /// The `tunables_json` of `Builder.build`: the web's `finOpts().tunables` (`web/ui/finbuild.js:139-147`), the
    /// module state the page reads from its controls and the Worker gets in the request. Values are what the page's
    /// modules hold after its own clamps: `propGap` 0.1...0.4 (`ui/settings.js:248`), the custom pad's numbers to
    /// their fields' ranges (`:139-142`), `tallRibsHeight` in mm from the 5...200 cm field (`:267`), `pinDepth` to
    /// 0.05...1 (`fins.js:124`). `padH` and `padGrab` are the fixed Sure-hold numbers the page keeps now the PLA/PETG
    /// profile is gone (`fins/config.js:16`, `fins/pad.js:14`). With the pad off the page keeps its last style, so a
    /// fresh page's `auto` is sent; a style other than Custom carries the page's untouched `padCustom`
    /// (`fins/pad.js:55`).
    func tunablesJSON() -> String {
        func clamp(_ v: Double, _ lo: Double, _ hi: Double) -> Double { min(hi, max(lo, v)) }
        let custom = bedPad == .custom
        let padCustom: [(String, Double)] = custom
            ? [("h", clamp(padThickness, 0.1, 1)), ("gap", clamp(padGap, 0, 0.5)),
               ("grip", clamp(padGrip, -0.2, 0.3)), ("margin", clamp(padSpread, 1, 12))]
            : [("h", 0.5), ("gap", 0), ("grip", 0.05), ("margin", 4)]
        let iface: String = switch interfaceMaterial { case .off: "false"; case .flat: "\"flat\""; case .all: "\"all\"" }
        let pad = padCustom.map { "\"\($0.0)\":\(Self.number($0.1))" }.joined(separator: ",")
        let o: [(String, String)] = [
            ("roundFeet", roundedFeet ? "true" : "false"),
            ("padH", Self.number(0.5)),
            ("padGrab", Self.number(0.05)),
            ("padStyle", "\"\(bedPad == .off ? "auto" : bedPad.rawValue)\""),
            ("padCustom", "{\(pad)}"),
            ("propGap", Self.number(clamp(gap, 0.1, 0.4))),
            ("tallRibsHeight", Self.number(clamp(tallRibsHeight, 5, 200) * 10)),
            ("cutout", "\"\(cutout.rawValue)\""),
            ("iface", iface),
            ("tineDir", "\"\(tineDirection.rawValue)\""),
            ("pinDepth", Self.number(clamp(pinDepth, 0.05, 1))),
            ("nozzle", Self.number(nozzle)),
            ("wallLines", String(wallLines)),
        ]
        return "{" + o.map { "\"\($0.0)\":\($0.1)" }.joined(separator: ",") + "}"
    }
}
