import Foundation

/// What `ui/readout.js` reads of a build's `built` object (`buildFins`'s result, `summary_json()` here).
/// Field names are the JS's. Only the fields a readout uses are here.
struct ReadoutBuilt: Decodable, Equatable, Sendable {
    struct Sway: Decodable, Equatable, Sendable {
        var count = 0, tines = 0, skipped = 0, cross = 0
        var reason: String?
    }
    struct Seating: Decodable, Equatable, Sendable { var kind: String }
    struct Pad: Decodable, Equatable, Sendable {
        var style: String
        var autoSure: Bool
        var smallFoot: Bool
        var outline: Double

        init(style: String, autoSure: Bool, smallFoot: Bool, outline: Double) {
            (self.style, self.autoSure, self.smallFoot, self.outline) = (style, autoSure, smallFoot, outline)
        }
        /// `outline` is null when the pad has no first-layer edge to measure; JS reads `null < 1` as true.
        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            style = try c.decode(String.self, forKey: .style)
            autoSure = try c.decode(Bool.self, forKey: .autoSure)
            smallFoot = try c.decode(Bool.self, forKey: .smallFoot)
            outline = try c.decodeIfPresent(Double.self, forKey: .outline) ?? 0
        }
        private enum CodingKeys: String, CodingKey { case style, autoSure, smallFoot, outline }
    }
    /// `islandsBuild`: what the midair-starts card is told. `all` is a Full build with no fill pass (every start is unwalled).
    struct IslandsBuild: Decodable, Equatable, Sendable {
        var bare: Int?
        var capped: Bool?
        var all: Bool?
    }
    struct Bounds: Decodable, Equatable, Sendable {
        var min: [Double], max: [Double]
    }
    struct Floating: Decodable, Equatable, Sendable { var drop: Double }
    struct Fill: Decodable, Equatable, Sendable {
        var walls: Int
        var bareBefore: Double
        var unservedArea: Double
        var capped: Bool
    }
    struct CurvedFill: Decodable, Equatable, Sendable {
        var walls: Int
        var unservedArea: Double
        var capped: Bool
    }
    struct Bare: Decodable, Equatable, Sendable {
        var area: Double, total: Double, coverage: Double
    }
    struct Rejected: Decodable, Equatable, Sendable {
        var blocked = 0, tooFewTines = 0, sites = 0
    }
    private struct Id: Decodable {}

    /// `auto` or `full` (Draw has its own readout, M6).
    var mode = "auto"
    /// `built.fins.length`
    var finCount = 0
    var propCount = 0, braceCount = 0, tines = 0, tallRibs = 0
    var sway: Sway?
    var seating: Seating?
    var pad: Pad?
    var floating: [Floating] = []
    var sagRisk = false
    var fill: Fill?
    var curvedFill: CurvedFill?
    var curvedDropped = false
    var bare: Bare?
    var unserved = 0.0
    var smallHoleRegions = 0
    var skipped: [String: Int] = [:]
    var patchStats: [String: Int] = [:]
    var patchCount = 0
    var rejected = Rejected()
    /// `summary_json()`'s `finVolumeMm3` / `padVolumeMm3`: the grams receipt's volumes (`meshVolumeMM3`)
    var finVolumeMm3 = 0.0
    var padVolumeMm3 = 0.0
    /// `fillReach(bare.coverage)` in mm; nil with `bare`
    var fillReach: Double?
    /// The extent of the fin and pad vertices in print space; nil when there are none
    var addedBounds: Bounds?
    /// nil unless the mode is Full
    var islandsBuild: IslandsBuild?

    init() {}

    /// From `BuildResult.summary_json()` (docs/FFI.md): the same keys the JS `built` has, plus `hasPad`. The
    /// record count is `records_json().fins.count` (the summary has no per-fin list).
    init(summaryJSON: String, finCount: Int) throws {
        self = try JSONDecoder().decode(ReadoutBuilt.self, from: Data(summaryJSON.utf8))
        self.finCount = finCount
    }

    private enum Key: String, CodingKey {
        case mode, fins, propCount, braceCount, tines, tallRibs, sway, seating, pad, floating, sagRisk, fill, curvedFill
        case curvedDropped, bare, unserved, smallHoleRegions, skipped, patchStats, patchCount, rejected
        case hasPad, finVolumeMm3, padVolumeMm3, fillReach, addedBounds, islandsBuild
    }

    /// A key missing from the JSON takes its default, as `undefined` reads in the JS (`built.skipped?.onPart`).
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        mode = try c.decodeIfPresent(String.self, forKey: .mode) ?? "auto"
        finCount = try c.decodeIfPresent([Id].self, forKey: .fins)?.count ?? 0
        propCount = try c.decodeIfPresent(Int.self, forKey: .propCount) ?? 0
        braceCount = try c.decodeIfPresent(Int.self, forKey: .braceCount) ?? 0
        tines = try c.decodeIfPresent(Int.self, forKey: .tines) ?? 0
        tallRibs = try c.decodeIfPresent(Int.self, forKey: .tallRibs) ?? 0
        sway = try c.decodeIfPresent(Sway.self, forKey: .sway)
        seating = try c.decodeIfPresent(Seating.self, forKey: .seating)
        pad = try c.decodeIfPresent(Pad.self, forKey: .pad)
        if pad == nil, try c.decodeIfPresent(Bool.self, forKey: .hasPad) == true {
            pad = Pad(style: "", autoSure: false, smallFoot: false, outline: 0)
        }
        floating = try c.decodeIfPresent([Floating].self, forKey: .floating) ?? []
        sagRisk = try c.decodeIfPresent(Bool.self, forKey: .sagRisk) ?? false
        fill = try c.decodeIfPresent(Fill.self, forKey: .fill)
        curvedFill = try c.decodeIfPresent(CurvedFill.self, forKey: .curvedFill)
        curvedDropped = try c.decodeIfPresent(Bool.self, forKey: .curvedDropped) ?? false
        bare = try c.decodeIfPresent(Bare.self, forKey: .bare)
        unserved = try c.decodeIfPresent(Double.self, forKey: .unserved) ?? 0
        smallHoleRegions = try c.decodeIfPresent(Int.self, forKey: .smallHoleRegions) ?? 0
        skipped = try c.decodeIfPresent([String: Int].self, forKey: .skipped) ?? [:]
        patchStats = try c.decodeIfPresent([String: Int].self, forKey: .patchStats) ?? [:]
        patchCount = try c.decodeIfPresent(Int.self, forKey: .patchCount) ?? 0
        rejected = try c.decodeIfPresent(Rejected.self, forKey: .rejected) ?? Rejected()
        finVolumeMm3 = try c.decodeIfPresent(Double.self, forKey: .finVolumeMm3) ?? 0
        padVolumeMm3 = try c.decodeIfPresent(Double.self, forKey: .padVolumeMm3) ?? 0
        fillReach = try c.decodeIfPresent(Double.self, forKey: .fillReach)
        addedBounds = try c.decodeIfPresent(Bounds.self, forKey: .addedBounds)
        islandsBuild = try c.decodeIfPresent(IslandsBuild.self, forKey: .islandsBuild)
    }
}

/// The page state around a build that the readout reads (`ui/readout.js` imports).
struct ReadoutContext: Decodable, Equatable, Sendable {
    /// `finMode`
    var mode = "auto"
    /// `removedIds.size`
    var removed = 0
    /// `lastResult.regions.length`
    var regions = 0
    /// `PROP.tineDir`: `horizontal` or `vertical` (resin pins)
    var tineDir = "horizontal"
    /// `PAD.style`: `auto`, `light`, `sure` or `custom`
    var padStyle = "auto"
    /// `PAD.custom.gap`
    var padCustomGap = 0.0
    /// `PROP.iface`: `flat`, `all`, or nil for off
    var iface: String?
    /// The build's time in ms; nil when the readout reruns without a rebuild (the timing line is left alone).
    var ms: Double?
    /// `analysisTiming`: `"5 ms · weld 7 ms"`
    var analysisTiming = ""
    /// `activeAdded().length > 0`: the fin + pad triangles exist
    var hasAdded = false
    /// `meshVolumeMM3(activeAdded())`: |signed volume| of fins + pad, mm^3 (the engine's `finVolumeMm3 + padVolumeMm3`)
    var volumeMm3 = 0.0
    /// `activeWalls().length > 0`
    var wallsPresent = false
    /// some wall vertex is tagged as an interface crest (`v[3] === 1`)
    var crestPresent = false
    /// `fillReach(built.bare.coverage)` in mm, which the engine owns (`fins/fill.js`, `FILL.denseReach`)
    var fillReach: Double?

    init() {}
}

/// Everything the Fins row, the Bed pad row, the notes under them, the (i) popover, the timing line and the grams
/// receipt show (`updateReadout` -> `updateFinReadout` and `updateReceipt`, `ui/readout.js:160-452`).
struct FinReadout: Equatable, Sendable {
    /// `#s-fins`
    var fins = ""
    /// `#s-fins` is styled as a warning
    var finsWarn = false
    /// `#s-pad`
    var pad = ""
    /// `#s-fin-note`: the must-see lines, joined `". "` with a final `"."`
    var lead = ""
    /// `#s-fin-info`'s tooltip: the how-it-works text behind the (i), or empty (the button is hidden)
    var info = ""
    /// `#s-time`, or nil when it is left as it was
    var time: String?
    /// `#r-grams` (`"4.2 g"`), or nil when the receipt is hidden
    var grams: String?

    /// The English text for a Full / Auto build (`updateFinReadout`).
    static func make(_ built: ReadoutBuilt?, _ ctx: ReadoutContext) -> FinReadout {
        var r = FinReadout()
        guard let b = built else {
            r.fins = "—"
            r.pad = "—"
            return r
        }
        let pins = ctx.tineDir == "vertical"
        func tineCount(_ n: Int) -> String { pins ? JS.tn("{n} pin", "{n} pins", n) : JS.tn("{n} tine", "{n} tines", n) }

        r.pad = padStatus(b)
        let n = b.finCount
        // Named apart so the readout is honest: the support fins sit on the overhangs (tined when the toggle is
        // on), the props are the fallback under ledges too flat to take a fin.
        var seg: [String] = []
        if b.braceCount != 0 {
            seg.append(JS.tn("{n} support fin", "{n} support fins", b.braceCount) + (b.tines != 0 ? " · " + tineCount(b.tines) : ""))
        }
        if b.propCount != 0 { seg.append(JS.tn("{n} prop", "{n} props", b.propCount)) }
        let autoTxt = seg.joined(separator: " + ")
        let removedTxt = ctx.removed != 0 ? " (\(JS.t("{n} removed", ["n": String(ctx.removed)])))" : ""
        let ribTxt = b.tallRibs != 0 ? " · " + JS.tn("{n} wall rib", "{n} wall ribs", b.tallRibs) : ""
        var swayTxt = ""
        if let sw = b.sway, sw.count != 0 {
            swayTxt = (!autoTxt.isEmpty ? " + " : "") + JS.tn("{n} sway brace", "{n} sway braces", sw.count)
            if sw.tines != 0 { swayTxt += " · " + JS.t("{n} brace tines", ["n": String(sw.tines)]) }
            if sw.cross != 0 { swayTxt += " · " + JS.tn("{n} brace rib", "{n} brace ribs", sw.cross) }
        }
        let noneNeeded = n == 0 && ctx.removed == 0 && (b.sway?.count ?? 0) == 0 && onlySmallHoles(b, ctx)
        let text = autoTxt + ribTxt + swayTxt + removedTxt
        r.fins = !text.isEmpty ? text : (noneNeeded
            ? JS.tn("none needed: small hole left for bridging", "none needed: small holes left for bridging", b.smallHoleRegions)
            : "none possible")
        r.finsWarn = n == 0 && (b.sway?.count ?? 0) == 0 && !noneNeeded

        // `lead` = short + must-see, stays in the panel; `help` = how-it-works and how-to-fix, behind the (i).
        var lead: [String] = []
        var help: [String] = []
        if n == 0 {
            // Nothing placed -- the box names the outcome; the why goes in the (i).
            help.append(explainNoFins(b, ctx))
        } else {
            let brace = b.braceCount, prop = b.propCount
            if brace != 0 {
                help.append(b.tines != 0
                    ? (pins ? "The pins attach each wall to the part, as a resin print needs." : "The tines grab onto the part and bend away when you snap the supports off.")
                    : "The fins stand a hair off the part (0.2mm) so they pop off. Turn Tines on if you want them to grip.")
            }
            if prop != 0 && brace == 0 {
                help.append("These are plain props, not gripping fins. The overhangs here are too shallow or curved to stand a fin against, so there are no tines to add.")
            } else if prop != 0 {
                help.append(JS.tn("The {n} prop sit under overhangs too shallow to grip, so those get no tines.",
                                  "The {n} props sit under overhangs too shallow to grip, so those get no tines.", prop))
            }
        }
        // Worth saying even when something WAS placed: a point-balanced part is standing on the added pad and
        // nothing else, so the pad is load-bearing, not cosmetic.
        if n != 0 && b.seating?.kind == "point" {
            lead.append(b.pad != nil
                ? "this part balances on one point, so the bed pad is holding it. Print with the pad on"
                : "this part balances on one point with nothing under it. Turn the bed pad on, or rotate until it sits down")
        }
        let note = padNote(b, ctx)
        if !note.isEmpty { lead.append(note) }
        if !b.floating.isEmpty {
            let count = b.floating.count
            let drop = JS.toFixed1(b.floating[0].drop)
            lead.append(JS.tn("one piece of this part isn’t joined to the rest: it starts {drop} mm up, held only by supports. Check the model is one piece (a hole or cut may go right through)",
                              "{n} pieces of this part aren’t joined to the rest: the first starts {drop} mm up, held only by supports. Check the model is one piece (a hole or cut may go right through)",
                              count, ["drop": drop]))
        }
        if b.sagRisk {
            lead.append("coverage is below the anti-sag guide, so a broad overhang may sag between supports — nudge the slider right if the surface bows")
        }
        // Full coverage: what it added, and -- must-see, in the panel -- the red it couldn't reach, never hidden.
        let fl = b.fill
        // More than half the red still bare, and not a few slivers: a figure, a mini, an organic shape
        // (`MOSTLY_BARE`, `ui/readout.js:28`).
        let ba = b.bare
        let pct = ba.map { $0.total != 0 ? Swift.max(1, Int(JS.round(100 * $0.area / $0.total))) : 0 } ?? 0
        let mostlyBare: Bool = {
            guard let ba, n > 0, ba.area >= 25, ba.area > 0.5 * ba.total else { return false }
            return fl != nil || (ba.coverage >= 0.5)
        }()
        if let fl {
            if fl.walls != 0 {
                help.append(JS.tn("Full coverage added {n} wall under overhangs Auto left bare.", "Full coverage added {n} walls under overhangs Auto left bare.", fl.walls))
            } else if fl.bareBefore < 1 && n != 0 {
                help.append("Auto already reaches every overhang here, so Full coverage added nothing.")
            }
            let bare = JS.round(fl.unservedArea)
            if fl.capped {
                lead.append(JS.t("Full coverage stopped at its wall limit with {area} mm² of overhang still bare — rotate the part, or add a wall by hand", ["area": JS.number(bare)]))
            } else if bare >= 1 && !mostlyBare {
                lead.append(JS.t("{area} mm² of overhang no wall can reach this way up — tilt the part, or add a wall by hand", ["area": JS.number(bare)]))
            }
        }
        // Curved walls in Auto: the fill pass under the regions a forced curve won.
        if let cf = b.curvedFill {
            if cf.walls != 0 {
                help.append(JS.tn("Curved walls: added {n} wall under overhang the curves left bare.", "Curved walls: added {n} walls under overhang the curves left bare.", cf.walls))
            }
            let bare = JS.round(cf.unservedArea)
            if cf.capped {
                lead.append(JS.t("Curved walls stopped at the wall limit with {area} mm² of overhang still bare — untick it, or switch to Full coverage", ["area": JS.number(bare)]))
            } else if bare >= 1 {
                lead.append(JS.t("{area} mm² of overhang no wall can reach this way up — tilt the part, or add a wall by hand", ["area": JS.number(bare)]))
            }
        }
        if mostlyBare {
            lead.append(fl != nil
                ? JS.t("Lots of small or tucked-in overhang here ({pct} % still bare in Full coverage). For figures and minis, slicer tree supports often do better.", ["pct": String(pct)])
                : JS.t("Lots of small or tucked-in overhang here ({pct} % bare). Try Full coverage; for figures and minis, slicer tree supports often do better.", ["pct": String(pct)]))
        } else if let ba, fl == nil, n != 0, JS.round(ba.area) >= 1 {
            help.append(JS.t("Auto leaves {area} mm² ({pct} %) of overhang more than {reach} mm from a wall — Full coverage adds walls there",
                             ["area": JS.number(JS.round(ba.area)), "pct": String(pct), "reach": JS.number(JS.round(ctx.fillReach ?? 0))]))
        }
        // Curved walls in Full: the straight build held more, so it was kept
        if b.curvedDropped {
            help.append("Curved walls: Full coverage does better here with straight walls, so it kept them. Switch to Auto for the curves.")
        }
        if b.unserved != 0 && fl == nil {
            help.append(JS.tn("{n} overhang is too shallow for a fin this way up. Tilt the part steeper so a fin can follow it (try Suggest orientation), or add a wall by hand.",
                              "{n} overhangs are too shallow for a fin this way up. Tilt the part steeper so a fin can follow it (try Suggest orientation), or add a wall by hand.",
                              Int(b.unserved)))
        }
        if b.smallHoleRegions != 0 {
            lead.append(JS.tn("{n} small-hole ceiling left unsupported for bridging — check the slicer preview and print settings.",
                              "{n} small-hole ceilings left unsupported for bridging — check the slicer preview and print settings.", b.smallHoleRegions))
        }
        // Plate only (#218) left these bare: say so, so the red isn't a mystery
        let onPart = b.skipped["onPart"] ?? 0
        if onPart != 0 {
            help.append(JS.tn("{n} support was cut short or left off because part of the model sits under the overhang and Plate only is on. Untick it to stand them on the part.",
                              "{n} supports were cut short or left off because part of the model sits under the overhang and Plate only is on. Untick it to stand them on the part.", onPart))
        }
        // Sway braces were asked for, so say what they did -- and why, if nothing.
        if let sw = b.sway {
            if sw.count == 0 {
                lead.append("no sway braces: \(sw.reason ?? "")")
            } else {
                help.append("The sway braces stand edge-on against the tall sides and are tied on by tines all the way up, so the top can’t drift or wobble as it prints.")
                if sw.skipped != 0 {
                    help.append(JS.tn("{n} brace spot was blocked by the part itself. Switch to Draw and click an upright side to place one by hand.",
                                      "{n} brace spots were blocked by the part itself. Switch to Draw and click an upright side to place one by hand.", sw.skipped))
                }
            }
        }
        if let iface = ifaceNote(ctx) { lead.append(iface) }

        r.lead = lead.isEmpty ? "" : lead.joined(separator: ". ") + "."
        r.info = help.filter { !$0.isEmpty }.joined(separator: " ")
        // ms is absent when the readout reruns without rebuilding: leave the timing line alone
        if let ms = ctx.ms { r.time = "\(ctx.analysisTiming) · fins \(JS.toFixed0(ms)) ms" }
        // The grams receipt: the mass of breakaway support the tool adds (`updateReceipt`, `readout.js:134-146`)
        if ctx.hasAdded { r.grams = "\(fmtGrams(ctx.volumeMm3 * Self.materialDensity / 1000)) g" }
        return r
    }

    /// The (i) on the grams receipt (`index.html:615`, docs/UI-MAP.md 4.3 `r-info`).
    static let receiptTip = "Grams assume PLA and count only the fins + pad the tool adds, printed near-solid, cross-checked against the builder's fin volume. In test prints, breakaway fins used 20–45% less plastic and printed ~30% faster than slicer supports."

    /// `materialDensity`: PLA's, g/cm^3 (`web/materials.js`, `ui/settings.js:281`).
    static let materialDensity = 1.24

    /// `fmtGrams`: one decimal under 9.95 g, else rounded (`ui/readout.js:131`).
    static func fmtGrams(_ g: Double) -> String { g < 9.95 ? JS.toFixed1(g) : JS.number(JS.round(g)) }

    // MARK: the pieces

    /// `padStatus` (`readout.js:282-288`).
    static func padStatus(_ b: ReadoutBuilt) -> String {
        guard let p = b.pad else { return "not needed" }
        return p.autoSure ? "Sure hold (small foot)" : "added"
    }

    /// `padNote` (`readout.js:295-310`): the Light pad grips by first-layer squish along the part's outline, so a
    /// part on a small foot has almost nothing for it.
    static func padNote(_ b: ReadoutBuilt, _ ctx: ReadoutContext) -> String {
        guard let p = b.pad, p.smallFoot else { return "" }
        let mm = p.outline < 1 ? "under 1 mm" : "\(JS.toFixed0(p.outline)) mm"
        if p.autoSure {
            return JS.t("this part meets the plate on a small foot ({mm} of first-layer edge), too little for a Light pad to grip, so Auto made it Sure hold, touching the part to hold it", ["mm": mm])
        }
        if ctx.padStyle == "light" {
            return JS.t("this part meets the plate on a small foot ({mm} of first-layer edge); a Light pad has almost nothing to grip. Auto or Sure hold holds it", ["mm": mm])
        }
        if ctx.padStyle == "custom" && ctx.padCustomGap > 0 {
            return JS.t("this part meets the plate on a small foot ({mm} of first-layer edge); a pad with a gap has almost nothing to grip. Sure hold, or Pad gap 0, holds it", ["mm": mm])
        }
        return ""
    }

    /// `ifaceNote` (`readout.js:178-183`): Flat contacts is on and there are walls, but no wall top got a crest.
    static func ifaceNote(_ ctx: ReadoutContext) -> String? {
        guard ctx.iface == "flat", ctx.wallsPresent, !ctx.crestPresent else { return nil }
        return "Interface material: no flat contacts here, so every support prints in one material"
    }

    /// `onlySmallHoles` (`readout.js:111-118`): an intentional empty result is different from a failed placement.
    static func onlySmallHoles(_ b: ReadoutBuilt, _ ctx: ReadoutContext) -> Bool {
        ctx.mode != "draw" && b.smallHoleRegions > 0
            && b.smallHoleRegions == ctx.regions
            && b.unserved == 0 && (b.skipped["onPart"] ?? 0) == 0 && b.floating.isEmpty
            && (b.sway?.reason ?? "").isEmpty
            && !(b.seating?.kind == "point" && b.pad == nil)
    }

    /// `explainNoFins` (`readout.js:38-109`): why did this part get no fins, in terms the user can act on?
    static func explainNoFins(_ b: ReadoutBuilt, _ ctx: ReadoutContext) -> String {
        // A part balanced on a point cannot be rescued by ANY support unless the bed pad is on to seat it.
        if b.seating?.kind == "point" && b.pad == nil {
            return "this part touches the plate at a single point, so it has nothing to stand on. Turn the bed pad on to seat it, or rotate until it sits down on a face or an edge"
        }
        // Plate only (#218) emptied it: in any mode, that is the reason to name
        if (b.skipped["onPart"] ?? 0) != 0 {
            return "part of the model sits under these overhangs, and Plate only is on — untick it to stand supports on the part, or rotate"
        }
        if onlySmallHoles(b, ctx) {
            return "recognised small-hole ceilings are left unsupported to keep their openings clear; check bridging in the slicer"
        }
        let st = b.patchStats
        if b.patchCount == 0 {
            // a cylinder or a mesh of small facets has no flat face wide enough
            return (st["tooNarrow"] ?? 0) > (st["notFlat"] ?? 0)
                ? "nothing flat and wide enough to stand a fin against — curved or finely faceted surfaces have no flat face to grip"
                : "no flat upright face on this part in this orientation"
        }
        if b.rejected.sites == 0 {
            if let tooHigh = st["tooHigh"], tooHigh != 0 {
                return JS.tn("{n} flat face found, but every one starts too far up the part — a fin would be mostly bare stilt. Rotate so a flat face runs down to the plate",
                             "{n} flat faces found, but every one starts too far up the part — a fin would be mostly bare stilt. Rotate so a flat face runs down to the plate", tooHigh)
            }
            return "no usable face in this orientation — try rotating"
        }
        if b.rejected.blocked != 0 {
            return "the part is in the way of every wall position on the faces it found — rotate, or switch to Draw and place one by hand"
        }
        return "the workable spots would put the fin inside the part — try rotating"
    }
}

extension JS {
    /// A whole number as JS prints it (`String(n)`).
    static func number(_ x: Double) -> String {
        if x == x.rounded(), abs(x) < 1e15 { return String(Int64(x)) }
        return "\(x)"
    }
}
