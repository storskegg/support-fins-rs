import Foundation

/// What the hand-placed supports tell the readout: the page state `ui/readout.js` reads from `ui/walls.js`
/// (`drawnWalls`, `drawMsg`, `selectedWall` / `selectedNote`, `drawShapeNow`, `drawShown`).
struct DrawReadoutInput: Codable, Equatable, Sendable {
    struct Wall: Codable, Equatable, Sendable {
        /// `w.kind`: `wall`, or `sway` for a brace
        var kind = "wall"
        /// `w.ok`
        var ok = true
        var tines = 0
        /// `w.info.braces`: a wall's face ribs
        var braces = 0
        /// `w.info.cross`: a brace's cross ribs
        var cross = 0
        /// `w.info.stilt`
        var stilt = 0.0
        /// `w.info.reason`
        var reason: String?

        init(kind: String = "wall", ok: Bool = true, tines: Int = 0, braces: Int = 0, cross: Int = 0, stilt: Double = 0, reason: String? = nil) {
            (self.kind, self.ok, self.tines, self.braces, self.cross, self.stilt, self.reason) = (kind, ok, tines, braces, cross, stilt, reason)
        }

        private enum Key: String, CodingKey { case kind, ok, tines, braces, cross, stilt, reason }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: Key.self)
            kind = try c.decodeIfPresent(String.self, forKey: .kind) ?? "wall"
            ok = try c.decodeIfPresent(Bool.self, forKey: .ok) ?? true
            tines = try c.decodeIfPresent(Int.self, forKey: .tines) ?? 0
            braces = try c.decodeIfPresent(Int.self, forKey: .braces) ?? 0
            cross = try c.decodeIfPresent(Int.self, forKey: .cross) ?? 0
            stilt = try c.decodeIfPresent(Double.self, forKey: .stilt) ?? 0
            reason = try c.decodeIfPresent(String.self, forKey: .reason)
        }
    }

    /// `drawnWalls`
    var walls: [Wall] = []
    /// `drawMsg`
    var msg = ""
    /// `selectedWall ? selectedNote() : null`
    var selectedNote: String?
    /// `drawShapeNow()`: `line` or `curve`
    var shape = "line"
    /// `drawShown()`
    var shown = false

    init() {}

    private enum Key: String, CodingKey { case walls, msg, selectedNote, shape, shown }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        walls = try c.decodeIfPresent([Wall].self, forKey: .walls) ?? []
        msg = try c.decodeIfPresent(String.self, forKey: .msg) ?? ""
        selectedNote = try c.decodeIfPresent(String.self, forKey: .selectedNote)
        shape = try c.decodeIfPresent(String.self, forKey: .shape) ?? "line"
        shown = try c.decodeIfPresent(Bool.self, forKey: .shown) ?? false
    }

    var okCount: Int { walls.filter(\.ok).count }
}

extension FinReadout {
    /// Draw mode's readout (`updateDrawReadout`, `ui/readout.js:206-264`). Reports the breakaway WALLS the user drew by
    /// hand, plus the pad / seating verdict from the build. When a drawn wall can't build it says WHY: silence-as-success
    /// is the exact bug M5's scoreboard was built on.
    static func makeDraw(_ built: ReadoutBuilt?, _ ctx: ReadoutContext) -> FinReadout {
        var r = FinReadout()
        let d = ctx.draw ?? DrawReadoutInput()
        let pins = ctx.tineDir == "vertical"
        func tineCount(_ n: Int) -> String { pins ? JS.tn("{n} pin", "{n} pins", n) : JS.tn("{n} tine", "{n} tines", n) }

        r.pad = built.map(padStatus) ?? "—"
        let ok = d.walls.filter(\.ok)
        let bad = d.walls.count - ok.count
        let tines = ok.reduce(0) { $0 + $1.tines }
        let braces = ok.filter { $0.kind == "sway" }.count
        let walls = ok.count - braces
        let ribs = ok.reduce(0) { $0 + ($1.kind == "sway" ? 0 : $1.braces) }
        let crosses = ok.reduce(0) { $0 + ($1.kind == "sway" ? $1.cross : 0) }
        var parts: [String] = []
        if walls != 0 {
            parts.append(JS.tn("{n} drawn wall", "{n} drawn walls", walls) + (ribs != 0 ? " · " + JS.tn("{n} wall rib", "{n} wall ribs", ribs) : ""))
        }
        if braces != 0 {
            parts.append(JS.tn("{n} sway brace", "{n} sway braces", braces)
                + (crosses != 0 ? " · " + JS.tn("{n} brace rib", "{n} brace ribs", crosses) : ""))
        }
        r.fins = !ok.isEmpty ? parts.joined(separator: " + ") + (tines != 0 ? " · " + tineCount(tines) : "") : JS.t("none yet")
        r.finsWarn = ok.isEmpty

        var lead: [String] = []
        var help: [String] = []
        if d.walls.isEmpty && d.msg.isEmpty {
            lead.append(d.shape == "curve"
                ? JS.t("Click the two ends of a curve on the overhang, then a point it runs through, to lay a curved breakaway wall under it")
                : JS.t("Click two points across an overhang (a line lands right where you draw it, red faces included) to lay a breakaway wall under it"))
        }
        if !ok.isEmpty {
            help.append(tines != 0
                ? (pins ? JS.t("The pins attach each wall to the part, as a resin print needs.") : JS.t("The tines grab onto the part and bend away when you snap the wall off."))
                : JS.t("Each wall stops a hair under the part (0.2mm) so it snaps off clean. Turn Tines on if you want it to grip the part."))
        }
        // A brace you place by hand is built even where Auto would refuse to stand one, so say what it is doing: below
        // its first tine it holds nothing and nothing holds it, which is worth knowing but is your call to make.
        let stilted = ok.filter { $0.kind == "sway" && $0.stilt > 20 }
        if !stilted.isEmpty {
            let tallest = stilted.map(\.stilt).max() ?? 0
            help.append(JS.tn("One brace stands up to {mm}mm before gripping the part — that much of it prints as a lone wall. Fine if it prints; rotate so that side reaches the plate if it wobbles.",
                              "{n} braces stand up to {mm}mm before gripping the part — that much of it prints as a lone wall. Fine if it prints; rotate so that side reaches the plate if it wobbles.",
                              stilted.count, ["mm": JS.number(JS.round(tallest))]))
        }
        if bad != 0 {
            let one = d.walls.first { !$0.ok }
            let reason = (one?.reason).flatMap { $0.isEmpty ? nil : " (\($0))" } ?? ""
            lead.append(JS.tn("{n} wall couldn’t build here{reason}. Undo, or redraw", "{n} walls couldn’t build here{reason}. Undo, or redraw", bad, ["reason": reason]))
        }
        if !d.msg.isEmpty { lead.append(d.msg) }
        if let note = d.selectedNote { lead.append(note) }
        if built?.seating?.kind == "point" {
            lead.append(built?.pad != nil
                ? JS.t("this part balances on one point, so the bed pad is holding it. Print with the pad on")
                : JS.t("this part balances on one point. Turn the bed pad on to seat it, or rotate until it sits down"))
        }
        if let b = built {
            let note = padNote(b, ctx)
            if !note.isEmpty { lead.append(note) }
        }
        if let iface = ifaceNote(ctx) { lead.append(iface) }

        r.lead = lead.isEmpty ? "" : lead.joined(separator: ". ") + "."
        r.info = help.filter { !$0.isEmpty }.joined(separator: " ")
        if let ms = ctx.ms { r.time = "\(ctx.analysisTiming) · \(JS.t("pad")) \(JS.toFixed0(ms)) ms" }
        if ctx.hasAdded { r.grams = "\(fmtGrams(ctx.volumeMm3 * Self.materialDensity / 1000)) g" }
        return r
    }
}
