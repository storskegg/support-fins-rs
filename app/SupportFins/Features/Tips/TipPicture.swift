import SwiftUI

/// What a tip-card picture is made of (`web/ui/tippics.js` builds SVG strings from these; `TipPictures.generated.swift`
/// holds them as data). `cls` is the web's style class (`tp-part`, `tp-sup`, ...).
enum TipPrim: Sendable {
    case rect(String, Double, Double, Double, Double, rx: Double, noFill: Bool = false)
    case line(String, Double, Double, Double, Double)
    case path(String, String, noFill: Bool = false)
    case ellipse(String, Double, Double, Double, Double, noFill: Bool = false)
}

enum TipPictures {
    /// The pictures are 240 x 130 (`svg()`, `tippics.js`).
    static let size = CGSize(width: 240, height: 130)

    static func has(_ key: String) -> Bool { all[key] != nil }
}

/// The picture of one setting, drawn the way `.tp-*` in `web/style.css` styles it: the part blue, supports green,
/// overhangs red, the plate grey, measurements in the accent. No words in it, so nothing here is translated.
struct TipPicture: View {
    let key: String

    var body: some View {
        Canvas { ctx, _ in
            for p in TipPictures.all[key] ?? [] { Self.draw(p, in: &ctx) }
        }
        .frame(width: TipPictures.size.width, height: TipPictures.size.height)
        .accessibilityHidden(true)
    }

    // MARK: style (`web/style.css:1254-1275`)

    private struct Style {
        var fill: Color?
        var stroke: Color?
        var width = 1.0
        var dash: [CGFloat] = []
        var opacity = 1.0
    }

    private static let green = Color(red: 0x3f / 255.0, green: 0xae / 255.0, blue: 0x6a / 255.0)
    private static let red = Color(red: 0xe0 / 255.0, green: 0x5a / 255.0, blue: 0x4f / 255.0)
    private static let blue = Color(red: 0x8f / 255.0, green: 0xa3 / 255.0, blue: 0xc8 / 255.0)
    private static let grey = Color(red: 0x5b / 255.0, green: 0x64 / 255.0, blue: 0x72 / 255.0)

    private static func style(_ cls: String) -> Style? {
        switch cls {
        case "tp-bed": Style(stroke: grey, width: 2)
        case "tp-part": Style(fill: blue)
        case "tp-sup": Style(fill: green)
        case "tp-over": Style(stroke: red, width: 3)
        case "tp-tine": Style(stroke: Color(red: 0x7b / 255.0, green: 0xe0 / 255.0, blue: 0xa0 / 255.0), width: 2)
        case "tp-dim", "tp-dim-head": Style(stroke: .accentColor, width: 1.5)
        case "tp-mark": Style(stroke: .secondary, width: 1, dash: [4, 3])
        case "tp-tool": Style(fill: grey)
        case "tp-layer": Style(stroke: Color(nsColor: .windowBackgroundColor), width: 1)
        case "tp-bead": Style(fill: green, stroke: Color(nsColor: .windowBackgroundColor), width: 1)
        case "tp-sup2": Style(fill: Color(red: 0x2b / 255.0, green: 0x8a / 255.0, blue: 0x52 / 255.0))
        case "tp-iface": Style(fill: Color(red: 0xa0 / 255.0, green: 0x70 / 255.0, blue: 0xe0 / 255.0))
        case "tp-hole": Style(fill: Color(nsColor: .windowBackgroundColor))
        case "tp-hand": Style(fill: Color.accentColor.opacity(0.35), stroke: .accentColor, width: 1.5)
        case "tp-ghost": Style(stroke: .secondary, width: 1.5, dash: [4, 3])
        case "tp-picked": Style(fill: green, stroke: .accentColor, width: 2.5)
        case "tp-x": Style(stroke: red, width: 3)
        case "tp-cursor": Style(fill: .white, stroke: Color(white: 0.07), width: 1)
        case "tp-amber": Style(stroke: Color(red: 0xf0 / 255.0, green: 0xa8 / 255.0, blue: 0x30 / 255.0), width: 4)
        case "tp-cone": Style(fill: red)
        case "tp-plan": Style(fill: blue, opacity: 0.45)
        case "tp-band": Style(stroke: green, width: 5)
        default: nil
        }
    }

    /// A style class of `web/style.css` this draws (`tp-*`).
    static func hasStyle(_ cls: String) -> Bool { style(cls) != nil }

    private static func draw(_ prim: TipPrim, in ctx: inout GraphicsContext) {
        let path: Path, cls: String, noFill: Bool
        var isLine = false
        switch prim {
        case let .rect(c, x, y, w, h, rx, nf):
            cls = c; noFill = nf
            path = rx > 0 ? Path(roundedRect: CGRect(x: x, y: y, width: w, height: h), cornerRadius: rx) : Path(CGRect(x: x, y: y, width: w, height: h))
        case let .line(c, x1, y1, x2, y2):
            cls = c; noFill = true; isLine = true
            var p = Path(); p.move(to: CGPoint(x: x1, y: y1)); p.addLine(to: CGPoint(x: x2, y: y2)); path = p
        case let .path(c, d, nf):
            cls = c; noFill = nf; path = SVGPath.parse(d)
        case let .ellipse(c, cx, cy, rx, ry, nf):
            cls = c; noFill = nf; path = Path(ellipseIn: CGRect(x: cx - rx, y: cy - ry, width: 2 * rx, height: 2 * ry))
        }
        let s = style(cls) ?? Style()
        if !noFill, !isLine, let f = s.fill { ctx.fill(path, with: .color(f.opacity(s.opacity))) }
        if let st = s.stroke {
            ctx.stroke(path, with: .color(st), style: StrokeStyle(lineWidth: s.width, lineCap: .butt, lineJoin: .miter, dash: s.dash))
        }
    }
}

/// The subset of SVG path data the pictures use: `M L Q A Z`, absolute.
enum SVGPath {
    static func parse(_ d: String) -> Path {
        var path = Path()
        let t = tokens(d)
        var i = 0
        var cur = CGPoint.zero, start = CGPoint.zero
        func num() -> Double { defer { i += 1 }; return i < t.count ? Double(t[i]) ?? 0 : 0 }
        var cmd: Character = "M"
        while i < t.count {
            if let c = t[i].first, c.isLetter { cmd = c; i += 1; if cmd == "Z" { path.closeSubpath(); cur = start; continue } }
            switch cmd {
            case "M":
                cur = CGPoint(x: num(), y: num()); start = cur; path.move(to: cur); cmd = "L"
            case "L":
                cur = CGPoint(x: num(), y: num()); path.addLine(to: cur)
            case "Q":
                let c = CGPoint(x: num(), y: num()), p = CGPoint(x: num(), y: num())
                path.addQuadCurve(to: p, control: c); cur = p
            case "A":
                let rx = num(), ry = num(), rot = num(), large = num() != 0, sweep = num() != 0
                let p = CGPoint(x: num(), y: num())
                addArc(&path, from: cur, to: p, rx: rx, ry: ry, rotation: rot, large: large, sweep: sweep)
                cur = p
            default:
                i += 1
            }
        }
        return path
    }

    private static func tokens(_ d: String) -> [String] {
        var out: [String] = [], num = ""
        func flush() { if !num.isEmpty { out.append(num); num = "" } }
        for ch in d {
            if ch.isLetter { flush(); out.append(String(ch)) }
            else if ch == " " || ch == "," { flush() }
            else if ch == "-" { flush(); num = "-" }
            else { num.append(ch) }
        }
        flush()
        return out
    }

    /// An SVG elliptical arc (endpoint form) as Béziers (SVG 1.1 implementation notes F.6).
    private static func addArc(_ path: inout Path, from p0: CGPoint, to p1: CGPoint, rx rx0: Double, ry ry0: Double,
                               rotation: Double, large: Bool, sweep: Bool) {
        var rx = abs(rx0), ry = abs(ry0)
        if rx == 0 || ry == 0 || p0 == p1 { path.addLine(to: p1); return }
        let phi = rotation * .pi / 180, cp = cos(phi), sp = sin(phi)
        let dx = (p0.x - p1.x) / 2, dy = (p0.y - p1.y) / 2
        let x1 = cp * dx + sp * dy, y1 = -sp * dx + cp * dy
        let lam = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry)
        if lam > 1 { rx *= lam.squareRoot(); ry *= lam.squareRoot() }
        let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1
        let den = rx * rx * y1 * y1 + ry * ry * x1 * x1
        var co = (max(0, num / den)).squareRoot()
        if large == sweep { co = -co }
        let cxp = co * rx * y1 / ry, cyp = -co * ry * x1 / rx
        let cx = cp * cxp - sp * cyp + (p0.x + p1.x) / 2, cy = sp * cxp + cp * cyp + (p0.y + p1.y) / 2
        func ang(_ ux: Double, _ uy: Double, _ vx: Double, _ vy: Double) -> Double {
            let a = atan2(ux * vy - uy * vx, ux * vx + uy * vy); return a
        }
        let th1 = ang(1, 0, (x1 - cxp) / rx, (y1 - cyp) / ry)
        var dth = ang((x1 - cxp) / rx, (y1 - cyp) / ry, (-x1 - cxp) / rx, (-y1 - cyp) / ry)
        if !sweep && dth > 0 { dth -= 2 * .pi } else if sweep && dth < 0 { dth += 2 * .pi }
        let n = max(1, Int((abs(dth) / (.pi / 2)).rounded(.up)))
        let step = dth / Double(n), k = 4.0 / 3.0 * tan(step / 4)
        var a = th1
        func pt(_ a: Double) -> (Double, Double, Double, Double) {
            let ca = cos(a), sa = sin(a)
            let x = rx * ca, y = ry * sa, dx = -rx * sa, dy = ry * ca
            return (cp * x - sp * y + cx, sp * x + cp * y + cy, cp * dx - sp * dy, sp * dx + cp * dy)
        }
        for _ in 0..<n {
            let (ax, ay, adx, ady) = pt(a), (bx, by, bdx, bdy) = pt(a + step)
            path.addCurve(to: CGPoint(x: bx, y: by), control1: CGPoint(x: ax + k * adx, y: ay + k * ady), control2: CGPoint(x: bx - k * bdx, y: by - k * bdy))
            a += step
        }
    }
}
