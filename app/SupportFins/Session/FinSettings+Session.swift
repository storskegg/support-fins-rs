import Foundation
import SupportFinsCore

/// The settings a session carries (`web/ui/session.js`, `web/ui/history.js` `FORM_IDS`): the web's fields by id, as
/// they hold their values -- a checkbox's boolean, or the field's text -- less the viewer's own (`VIEWER_IDS`) and the
/// printer's nozzle, which a session neither saves nor applies. The mapping is the only place that knows both
/// spellings; the engine (`sf_io::session`) owns what a field can take.
extension FinSettings {
    /// A setting as its field holds it.
    enum SessionValue: Equatable {
        case flag(Bool)
        case text(String)
    }

    /// `FORM_IDS` less `VIEWER_IDS`, in the web's order (the writer keeps it: the session text is byte-for-byte the web's).
    static let sessionIDs = [
        "thr", "tines", "tine-density", "tine-dir", "pin-depth", "layer-height", "gap", "bed-pad", "pad-h", "pad-gap",
        "pad-grip", "pad-margin", "sway", "sway-from", "sway-spacing", "sway-depth", "sway-cross-height", "full-brace-depth",
        "rounded-feet", "wall-lines", "cutout", "iface", "coverage", "plate-only", "long-walls", "separate-walls", "tall-ribs",
        "tall-ribs-height", "curved-walls",
    ]

    /// `sessionForm()` (`ui/history.js:76`): every field's value, as the field would hold it.
    var sessionForm: [(id: String, value: SessionValue)] {
        func n(_ x: Double) -> SessionValue { .text(jsNumberText(x: x)) }
        return Self.sessionIDs.map { id in
            switch id {
            case "thr": (id, n(threshold))
            case "tines": (id, .flag(tines))
            case "tine-density": (id, n(tineDensity))
            case "tine-dir": (id, .text(tineDirection.rawValue))
            case "pin-depth": (id, n(pinDepth))
            case "layer-height": (id, n(layerHeight))
            case "gap": (id, n(gap))
            case "bed-pad": (id, .text(bedPad.rawValue))
            case "pad-h": (id, n(padThickness))
            case "pad-gap": (id, n(padGap))
            case "pad-grip": (id, n(padGrip))
            case "pad-margin": (id, n(padSpread))
            case "sway": (id, .flag(sway))
            case "sway-from": (id, n(swayFrom))
            case "sway-spacing": (id, n(swaySpacing))
            case "sway-depth": (id, n(swayDepth))
            case "sway-cross-height": (id, n(swayCrossHeight))
            case "full-brace-depth": (id, .flag(fullBraceDepth))
            case "rounded-feet": (id, .flag(roundedFeet))
            case "wall-lines": (id, .text(String(wallLines)))
            case "cutout": (id, .text(cutout.rawValue))
            case "iface": (id, .text(interfaceMaterial.rawValue))
            case "coverage": (id, n(coverage))
            case "plate-only": (id, .flag(plateOnly))
            case "long-walls": (id, .flag(longWalls))
            case "separate-walls": (id, .flag(separateWalls))
            case "tall-ribs": (id, .flag(tallRibs))
            case "tall-ribs-height": (id, n(tallRibsHeight))
            default: (id, .flag(curvedWalls))   // "curved-walls"
            }
        }
    }

    /// `restoreForm` (`ui/history.js`): write the fields a session restores. A field this app has no setting for, or a
    /// value of the wrong kind, is skipped (the engine has already dropped what the field cannot take).
    mutating func apply(session fields: [SessionField]) {
        for f in fields {
            let flag = f.flag, text = f.text
            let num = text.flatMap { Double($0) }
            switch f.id {
            case "thr": if let num { threshold = num }
            case "tines": if let flag { tines = flag }
            case "tine-density": if let num { tineDensity = num }
            case "tine-dir": if let v = text.flatMap(TineDirection.init(rawValue:)) { tineDirection = v }
            case "pin-depth": if let num { pinDepth = num }
            case "layer-height": if let num { layerHeight = num }
            case "gap": if let num { gap = num }
            case "bed-pad": if let v = text.flatMap(BedPad.init(rawValue:)) { bedPad = v }
            case "pad-h": if let num { padThickness = num }
            case "pad-gap": if let num { padGap = num }
            case "pad-grip": if let num { padGrip = num }
            case "pad-margin": if let num { padSpread = num }
            case "sway": if let flag { sway = flag }
            case "sway-from": if let num { swayFrom = num }
            case "sway-spacing": if let num { swaySpacing = num }
            case "sway-depth": if let num { swayDepth = num }
            case "sway-cross-height": if let num { swayCrossHeight = num }
            case "full-brace-depth": if let flag { fullBraceDepth = flag }
            case "rounded-feet": if let flag { roundedFeet = flag }
            case "wall-lines": if let text, let n = Int(text) { wallLines = n }
            case "cutout": if let v = text.flatMap(Cutout.init(rawValue:)) { cutout = v }
            case "iface": if let v = text.flatMap(InterfaceMaterial.init(rawValue:)) { interfaceMaterial = v }
            case "coverage": if let num { coverage = num }
            case "plate-only": if let flag { plateOnly = flag }
            case "long-walls": if let flag { longWalls = flag }
            case "separate-walls": if let flag { separateWalls = flag }
            case "tall-ribs": if let flag { tallRibs = flag }
            case "tall-ribs-height": if let num { tallRibsHeight = num }
            case "curved-walls": if let flag { curvedWalls = flag }
            default: break
            }
        }
    }
}
