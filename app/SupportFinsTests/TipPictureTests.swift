import SwiftUI
import Testing
@testable import Support_Fins

/// The tip cards (`web/ui/tips.js`, `web/ui/tippics.js`): every picture the web has, drawn from the generated primitives.
@MainActor
@Suite("Tip pictures")
struct TipPictureTests {
    /// The picture keys of docs/UI-MAP.md section 6 (`data-tip-pic`), and `pin-spacing` (`ui/settings.js:183`).
    static let keys = ["printer", "nozzle", "layer-height", "gap", "iface", "tines", "tine-density", "tine-dir", "pin-depth", "cutout",
                       "add-walls", "remove-fins", "restore-fins", "draw-remove", "rounded-feet", "bed-pad", "pad-h", "pad-gap",
                       "pad-grip", "pad-margin", "wall-lines", "tall-ribs", "tall-ribs-height", "sway", "sway-from", "sway-spacing",
                       "sway-depth", "full-brace-depth", "sway-cross-height", "coverage", "plate-only", "separate-walls",
                       "long-walls", "curved-walls", "highlight-small", "show-islands", "pin-spacing"]

    @Test("every picture of the web is there, and is made of known styles")
    func allPictures() {
        #expect(Set(Self.keys) == Set(TipPictures.all.keys))
        for (key, prims) in TipPictures.all {
            #expect(!prims.isEmpty, "\(key) is empty")
            for p in prims {
                let cls: String = switch p {
                case let .rect(c, _, _, _, _, _, _), let .line(c, _, _, _, _), let .path(c, _, _), let .ellipse(c, _, _, _, _, _): c
                }
                #expect(TipPicture.hasStyle(cls), "\(key) uses the unknown class \(cls)")
            }
        }
    }

    @Test("path data: lines, closing and arcs")
    func paths() {
        let box = SVGPath.parse("M10 20 L30 20 L30 50 Z").boundingRect
        #expect(box == CGRect(x: 10, y: 20, width: 20, height: 30))
        // `curved-walls`: a quarter circle of radius 34 round (178, 65) from (178, 31) to (212, 65)
        let arc = SVGPath.parse("M178 31 A34 34 0 0 1 212 65")
        #expect(arc.currentPoint.map { abs($0.x - 212) < 1e-6 && abs($0.y - 65) < 1e-6 } == true)
        let b = arc.boundingRect
        #expect(abs(b.minX - 178) < 0.5 && abs(b.maxX - 212) < 0.5 && abs(b.minY - 31) < 0.5 && abs(b.maxY - 65) < 0.5)
        // a quadratic curve, as `restore-fins` draws
        #expect(SVGPath.parse("M108 82 Q120 66 132 82").currentPoint == CGPoint(x: 132, y: 82))
    }

    /// The tooltip each card carries with a picture, where it is not `Tip.<camelCase key>`: the web's `title` of the element
    /// with that `data-tip-pic` (`index.html`, `ui/settings.js` for the coverage and pin-spacing swaps).
    static let otherTooltips = [
        "printer": #"JS.t("Your printer: nozzle, layer height, support gap, interface material. Click to change.")"#,
        "add-walls": "DrawTip.augment",
        "draw-remove": "DrawTip.removeSelected",
        "remove-fins": #"JS.t("Click a fin to remove just that one. Esc or right-click cancels.")"#,
        "restore-fins": #"JS.t("Bring back every fin removed in this orientation.")"#,
        "coverage": "v.coverageDisabled ? Self.coverageTipFull : Tip.coverage",
    ]

    /// Each `.tipCard(pic:)` / `pic:` call site pairs its picture with that setting's own tooltip (a merge once mixed the
    /// rows' wrappers), and every picture is used. Reads the app's sources, as the UI-MAP tooltip test reads the map.
    @Test("every picture is shown with its own setting's tooltip")
    func picturesPairWithTheirTooltips() throws {
        let app = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .appending(path: "SupportFins")
        let files = try #require(FileManager.default.enumerator(at: app, includingPropertiesForKeys: nil))
        let pic = try Regex(#"pic: "([a-z-]+)""#)
        var used: Set<String> = []
        for case let url as URL in files where url.pathExtension == "swift" && !url.path.contains("/Features/Tips/") {
            for line in try String(contentsOf: url, encoding: .utf8).split(separator: "\n") {
                for m in line.matches(of: pic) {
                    let key = String(m.output[1].substring!)
                    used.insert(key)
                    let camel = key.split(separator: "-").enumerated()
                        .map { $0.offset == 0 ? String($0.element) : $0.element.capitalized }.joined()
                    let tip = Self.otherTooltips[key] ?? "Tip.\(camel)"
                    // whole: `Tip.sway` must not pass for `Tip.swayFrom`
                    let whole = try Regex(NSRegularExpression.escapedPattern(for: tip) + "(?![A-Za-z0-9_])")
                    #expect(line.contains(whole), "\(url.lastPathComponent): the \(key) picture goes with \(tip)")
                }
                // the grip row swaps picture and tooltip with the tine direction (`ui/settings.js:183`)
                if line.contains(#""pin-spacing""#) {
                    #expect(line.contains(#"settings.pins ? "pin-spacing" : "tine-density""#)
                            && line.contains("settings.pins ? Self.pinSpacingTip : Tip.tineDensity"), "the grip row's swap")
                    used.formUnion(["pin-spacing", "tine-density"])
                }
            }
        }
        #expect(used == Set(Self.keys))
    }

    @Test("a card draws: picture over text")
    func cardRenders() throws {
        for key in ["sway", "curved-walls", "cutout"] {
            let r = ImageRenderer(content: TipCard(pic: key, text: Tip.sway))
            let image = try #require(r.nsImage)
            #expect(image.size.width >= 240 && image.size.height > 130)
        }
        // a setting without a picture: the text alone
        let plain = try #require(ImageRenderer(content: TipCard(pic: nil, text: Tip.finMode)).nsImage)
        #expect(plain.size.height < 130)
        // as wide as its text (the web's `.plain`, `max-content`), and a long one wraps at a card's width
        #expect(plain.size.width < TipCard.width - 40)
        let long = try #require(ImageRenderer(content: TipCard(pic: nil, text: Tip.cutout)).nsImage)
        #expect(long.size.width == TipCard.width)
    }
}
