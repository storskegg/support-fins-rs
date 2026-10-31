import Foundation
import SupportFinsCore

/// A test print (`COUPONS`, `web/ui/calibrate.js:19-61`), in print order: angle first, since it decides which faces get
/// support. The strings are the web's, word for word; they go through `JS.t` where they are shown, as the page's `t()`.
struct Coupon: Identifiable, Equatable {
    let name: String
    let file: String
    /// What setting it sets.
    let sets: String
    /// A note on how to print it, for the ones that have one.
    let note: String?
    /// What to read off the print.
    let how: String

    var id: String { file }
}

/// The Plate ties print (issue #50, slice 3): the app's own and optional, listed after the web's coupons. It is not a
/// bundled file: the engine makes it when it is saved (`export_tie_calibration`), for the printer picked in the
/// Printer section and with the window's fin settings, so its tie thicknesses follow the printer.
struct TiePrintInfo {
    let name: String
    let file: String
    let sets: String
    let note: String
    let how: String
    /// The row's and its button's accessibility suffix.
    var id: String { "plate-ties" }
}

/// The estimates of `coupons.json` (`prototype/calibration/estimate.py`): each coupon's print time and filament, and
/// the slicer profile they came from.
struct CouponEstimates: Decodable, Equatable {
    struct Estimate: Decodable, Equatable {
        var minutes: Int
        var grams: Double
        var sha256: String
    }

    var profile: String
    var coupons: [String: Estimate]
}

enum CalibrationCatalog {
    /// The page's `GUIDE` (`web/ui/calibrate.js:15`): docs/CALIBRATION.md.
    static let guide = URL(string: "https://github.com/gittrahan/support-fins/blob/main/docs/CALIBRATION.md")!

    /// The sheet's first line (`#calibrate-menu .calibrate-intro`, `index.html:140`).
    static let intro = "Print each with your usual profile and slicer supports off; the supports are in the file. Each sets one setting."

    static let coupons: [Coupon] = [
        Coupon(name: "Angle", file: "angle-coupon.stl",
               sets: "Sets the Overhang slider (top bar) · print this first",
               note: "No supports in this one: print it as it is.",
               how: "Seven ramps, 10° to 40°, measured up from the plate, so lower is harder. The shallowest ramp that came out clean (no droop, no curls) is your Overhang setting. The slider stops at 30°: if a lower ramp came out clean, set 30."),
        Coupon(name: "Gap", file: "gap-coupon.3mf",
               sets: "Sets Printer ▸ Edit ▸ Support gap",
               note: "Print at 0.2 mm layers with a 0.2 mm first layer and adaptive layer height off, or the gaps stop being whole layers.",
               how: "Ledges held 0.2, 0.4 and 0.6 mm below (1, 2 and 3 empty layers). Snap each wall off. Welded or tearing the ledge = too small; saggy or stringy underside = too big. Use the smallest gap that snaps off clean. The field stops at 0.4: if only 0.6 came clean, set 0.4 and tell us."),
        Coupon(name: "Grip", file: "grip-coupon.3mf",
               sets: "Sets Tines ▸ Tine grip",
               note: nil,
               how: "Four ramps built at OFF, LIGHT (the default), MID and FIRM. Check every wall is still standing, then snap them off. A wall leaned or came loose: go one step right. Marks where the tines were: go one step left. Use the lightest setting whose walls all held."),
        Coupon(name: "Pad", file: "pad-coupon.3mf",
               sets: "Sets More settings ▸ Bed pad ▸ Custom ▸ Pad gap",
               note: nil,
               how: "Six cubes on an edge, each held down only by a pad 0 to 0.3 mm off it. A cube came loose = gap too big; a pad won't peel or tears the edge = too small. Use the biggest gap whose cube stayed put, with Pad thickness 0.2, Pad grip 0 and Pad spread 4."),
        Coupon(name: "Cutouts", file: "cutout-coupon.3mf",
               sets: "Sets Cutouts (below Tines)",
               note: nil,
               how: "Five tall ledges, each held by walls cut with one style: NONE, DIAMOND, TRIANGLE, ARCH, LATTICE. Check every wall stood and every ledge printed flat, then snap the walls off. A wall that leaned, sagged at a hole or broke apart: that style is too open. Use the most open style whose walls all held; Arch and Lattice save about a third of the support plastic."),
        Coupon(name: "Bore", file: "bore-coupon.3mf",
               sets: "No setting yet: tells us how small a hole to support",
               note: nil,
               how: "A block with two sets of sideways holes, 3 to 12 mm across. In set A one wall runs along each hole, the way the site does it; in set X three walls run across it. Pull every wall out of an open end (an X wall slides out along the hole) and tell us which came out clean, which broke or stuck, and whether A or X left the smoother ceiling."),
        Coupon(name: "Sampler", file: "sampler-coupon.3mf",
               sets: "Sets nothing: print it last, to see what your parts will get",
               note: "Part and supports are two objects: don't Arrange or move one without the other.",
               how: "One part with every hard shape: ramps, a ball, a thin ledge, a flat table, a mushroom, a cave, sideways holes and an arch, supported in Full coverage. Snap every wall off and tell us which stuck or tore the part, and which undersides sagged. A few droopy layers on the ball and a rougher table near the spine are what we get too."),
    ]

    static let tiePrint = TiePrintInfo(
        name: "Plate ties", file: "plate-ties-coupon.3mf",
        sets: "Optional: sets Plate ties ▸ Contact thickness, for tall, thin parts printed side by side",
        note: "Made when you save it, for the printer picked under Printer and with your fin settings. It is one merged object, as a plate with ties exports.",
        how: "Three pairs of upright coins, each pair held by three ties of its own thickness, numbered on the coins' outer faces: 1 thinnest, 3 thickest. Check that every pair stood straight, then snap each pair apart. A tie that broke during the print or let its coins lean is too thin; one that tore the coin's face or left a stub is too thick. Use the thinnest that held and snapped off clean as your Contact thickness.")

    /// The print's three thicknesses on `printer`: "On Elegoo Saturn 4 Ultra 16K: 1 = 0.304 mm, 2 = 0.6 mm, 3 = 0.896 mm at
    /// the contact; 2 is the printer's default." Empty without three variants.
    static func tieVariantsLine(_ v: [TieCalibrationVariant], printer: String) -> String {
        guard v.count == 3 else { return "" }
        let mm = { (x: Double) in String(format: "%g", x) }
        let d = v.first(where: \.isDefault).map { String($0.label) } ?? "2"
        return JS.t("On {printer}: 1 = {one} mm, 2 = {two} mm, 3 = {three} mm at the contact; {default} is the printer's default.",
                    ["printer": printer, "one": mm(v[0].contactThickness), "two": mm(v[1].contactThickness),
                     "three": mm(v[2].contactThickness), "default": d])
    }

    /// `fmtTime`: 19 -> "19 min", 62 -> "1 h 2 min".
    static func fmtTime(_ min: Int) -> String {
        min < 60 ? "\(min) min" : "\(min / 60) h\(min % 60 != 0 ? " \(min % 60) min" : "")"
    }

    /// The row's suffix when it has an estimate (`plugin-v`): " · 19 min · 7 g".
    static func detail(_ e: CouponEstimates.Estimate) -> String {
        " · \(fmtTime(e.minutes)) · \(String(Int64(JS.round(e.grams)))) g"
    }

    /// The footer's note on where the numbers come from (`#calibrate-profile`), empty without estimates.
    static func profileNote(_ estimates: CouponEstimates?) -> String {
        guard let p = estimates?.profile, !p.isEmpty else { return "" }
        return JS.t("Times and filament from {profile}; yours will differ.", ["profile": p])
    }

    /// The bundled folder of the coupon files and `coupons.json` (`web/calibration/`).
    static func resourceURL(_ name: String, in bundle: Bundle = .main) -> URL? {
        bundle.url(forResource: name, withExtension: nil, subdirectory: "Calibration")
    }

    /// `coupons.json`, or nil when it is missing or unreadable ("offline: the downloads still work, just without estimates").
    static func loadEstimates(in bundle: Bundle = .main) -> CouponEstimates? {
        guard let url = resourceURL("coupons.json", in: bundle), let data = try? Data(contentsOf: url) else { return nil }
        return try? JSONDecoder().decode(CouponEstimates.self, from: data)
    }
}
