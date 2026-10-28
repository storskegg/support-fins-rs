import Foundation

/// The stats card's part rows (`ui/part.js` `report`, `paintOverhangs`, `shade`, `updateFit`): what the analysis of
/// the part in its current pose says. The numbers come from the engine (`Part.analyze`, the build's summary); this
/// only words them.
struct PartReadoutInput: Equatable, Sendable {
    var name = ""
    var triangles = 0
    /// The seated part's size, mm (`AnalysisSummary.size`)
    var size: (x: Double, y: Double, z: Double) = (0, 0, 0)
    /// `res.regions.length`: overhang regions big enough for a fin
    var regionCount = 0
    /// `res.rawRegionCount`: regions before the sliver filter (`regionCount` + the slivers)
    var rawRegionCount = 0
    var overArea = 0.0
    var bedArea = 0.0
    /// `holes.count`: small-hole ceilings a finished build left for bridging (`smallHoleRegions`)
    var smallHoleCount = 0
    /// `islands.length`: spots whose first layer starts in midair
    var islandCount = 0
    /// Display > Highlight small overhangs
    var highlightSmall = true
    /// `flatRegions`: the region count of the part as loaded (its flat baseline); nil before it is known
    var flatRegions: Int?
    /// `analyze`'s time and the topology weld's, ms
    var analysisMs = 0.0
    var weldMs = 0.0

    static func == (a: Self, b: Self) -> Bool {
        a.name == b.name && a.triangles == b.triangles && a.size == b.size && a.regionCount == b.regionCount
            && a.rawRegionCount == b.rawRegionCount && a.overArea == b.overArea && a.bedArea == b.bedArea
            && a.smallHoleCount == b.smallHoleCount && a.islandCount == b.islandCount && a.highlightSmall == b.highlightSmall
            && a.flatRegions == b.flatRegions && a.analysisMs == b.analysisMs && a.weldMs == b.weldMs
    }
}

/// A piece of the Overhangs row: text, or the amber swatch with its tooltip.
enum OverhangsSegment: Equatable, Sendable {
    case text(String)
    case swatch(help: String)
}

struct PartReadout: Equatable, Sendable {
    var triangles = ""
    /// `X × Y × Z mm`, one decimal (`part.js:449-450`)
    var size = ""
    var overhangs: [OverhangsSegment] = []
    /// green: nothing to support (`part.js:326`)
    var overhangsGood = false
    /// `res.overArea.toFixed(0) mm²`
    var area = ""
    var bed = ""
    /// under 1 mm² touches the plate
    var bedWarn = false
    /// `#s-flat-note`: empty hides it
    var flatNote = ""
    var flatNoteGood = false
    /// `#over-warn`
    var overWarn = ""
    /// `#s-time` before a build: `analysis ms · weld ms`
    var time = ""

    /// `MIN_REGION_AREA` (`web/overhangs.js`): the least overhang area a fin is built for, mm².
    static let minRegionArea = 12

    static func make(_ i: PartReadoutInput) -> PartReadout {
        var r = PartReadout()
        r.triangles = i.triangles.formatted()
        r.size = "\(JS.toFixed1(i.size.x)) × \(JS.toFixed1(i.size.y)) × \(JS.toFixed1(i.size.z)) mm"
        let dropped = i.rawRegionCount - i.regionCount
        // The slivers keep their own amber swatch, so the amber faces have a name even when this pose has no
        // region the tool supports (`part.js:303-326`).
        var seg: [OverhangsSegment] = [.text(i.regionCount == 0 ? "none" : JS.tn("{n} region", "{n} regions", i.regionCount))]
        if dropped > 0 {
            seg.append(.text(" (+"))
            seg.append(.swatch(help: JS.t("Amber: overhangs under {area} mm², too small for a fin", ["area": String(minRegionArea)])))
            seg.append(.text("\(JS.tn("{n} sliver", "{n} slivers", dropped)))"))
        }
        if i.smallHoleCount > 0 {
            seg.append(.text(" ("))
            seg.append(.swatch(help: "Amber after generation: small-hole ceilings left for bridging"))
            seg.append(.text(JS.tn("{n} small hole left for bridging", "{n} small holes left for bridging", i.smallHoleCount) + ")"))
        }
        r.overhangs = seg
        r.overhangsGood = i.regionCount == 0 && dropped <= 0
        r.area = "\(JS.toFixed0(i.overArea)) mm²"
        r.bed = "\(JS.toFixed0(i.bedArea)) mm²"
        r.bedWarn = i.bedArea < 1

        // Overhang warning card (`part.js:333-340`): the amber slivers print rough by surprise.
        if dropped > 0 && i.highlightSmall {
            r.overWarn = JS.tn(
                "⚠ {n} overhang shaded amber is too small for a fin (under {area} mm² each), so it prints unsupported this way up and may come out rough. Try Suggest orientation to point it up.",
                "⚠ {n} overhangs shaded amber are too small for a fin (under {area} mm² each), so they print unsupported this way up and may come out rough. Try Suggest orientation to point them up.",
                dropped, ["area": String(minRegionArea)])
        }

        // "Do you even need me?" -- the honest signal before the user turns fins on (`part.js:374-385`).
        if i.regionCount == 0 && i.islandCount > 0 {
            // no red, but a start in midair: not "no supports needed" (the midair-starts card says why)
        } else if i.regionCount == 0 {
            r.flatNote = dropped != 0 ? "Nothing big enough for a fin this way up." : "No supports needed this way up."
            r.flatNoteGood = true
        } else if i.flatRegions == 0 {
            r.flatNote = "This prints clean lying flat. You only need fins if you’re tilting it for strength."
        }
        r.time = "\(JS.toFixed0(i.analysisMs)) ms · weld \(JS.toFixed0(i.weldMs)) ms"
        return r
    }
}

/// "Does it fit the build volume -- including everything the tool ADDS?" (`updateFit`, `part.js:472-505`). The
/// bounds are numbers the engine gives (the seated part's size, the fin + pad triangles' extent); this compares them.
struct FitInput: Equatable, Sendable {
    /// The seated part's size, mm: it stands centred on its spot, from z 0.
    var size: (x: Double, y: Double, z: Double) = (0, 0, 0)
    /// The extent of the fins and pad: min and max x, y and max z; nil when none were added.
    var added: (x0: Double, x1: Double, y0: Double, y1: Double, z1: Double)?
    /// The build volume, mm
    var volume: (x: Double, y: Double, z: Double) = (256, 256, 256)
    /// Where the model stands on the plate (`liveXY`)
    var spot: (x: Double, y: Double) = (0, 0)
    /// A parked model dragged off the bed (`parkedOff`)
    var parkedOff = false
    /// Models overlap (`overlapping`)
    var clash = false

    static func == (a: Self, b: Self) -> Bool {
        a.size == b.size && a.added?.x0 == b.added?.x0 && a.added?.x1 == b.added?.x1 && a.added?.y0 == b.added?.y0
            && a.added?.y1 == b.added?.y1 && a.added?.z1 == b.added?.z1 && a.volume == b.volume && a.spot == b.spot
            && a.parkedOff == b.parkedOff && a.clash == b.clash
    }
}

struct FitReadout: Equatable, Sendable {
    var text: String
    var warn: Bool

    static func make(_ f: FitInput) -> FitReadout {
        let v = f.volume
        var x0 = -f.size.x / 2, x1 = f.size.x / 2, y0 = -f.size.y / 2, y1 = f.size.y / 2, z1 = f.size.z
        if let a = f.added {
            x0 = min(x0, a.x0); x1 = max(x1, a.x1); y0 = min(y0, a.y0); y1 = max(y1, a.y1); z1 = max(z1, a.z1)
        }
        let big = x1 - x0 > v.x || y1 - y0 > v.y || z1 > v.z
        let e = 1e-6
        // where it stands on the plate: a model that would fit can hang off, and so can a parked one dragged there
        let off = !big && (x0 + f.spot.x < -v.x / 2 - e || x1 + f.spot.x > v.x / 2 + e
            || y0 + f.spot.y < -v.y / 2 - e || y1 + f.spot.y > v.y / 2 + e || f.parkedOff)
        let text: String
        if big {
            text = f.added != nil ? "does not fit (with fins)" : "does not fit"
        } else if off && f.clash {
            text = "models overlap · hangs off the bed"
        } else if off {
            text = "hangs off the bed"
        } else if f.clash {
            text = "models overlap"
        } else {
            text = "fits"
        }
        return FitReadout(text: text, warn: big || off || f.clash)
    }
}
