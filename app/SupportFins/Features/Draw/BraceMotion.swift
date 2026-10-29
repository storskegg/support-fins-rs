import CoreGraphics
import Foundation
import Observation
import Viewer
import simd

/// The sway brace hover preview's motion (`web/ui/braceghost.js`; `DrawModel` decides WHAT it shows): the ghost
/// fades in and out instead of popping, a big jump in height cross-fades, a pointer slipping off the upright side
/// for a moment (an edge, a small facet) does not blink it, the label eases after the cursor, the cursor dot eases to
/// amber on a spot that cannot take a brace, and the rotate rings fade back while it is up (`web/ui/ringfade.js`).
/// Nothing here changes which brace a click stands.
///
/// Time is stepped by `advance(ms:)`, which the frame loop calls with the time since its last step; a test steps it
/// by hand.
@MainActor
@Observable
final class BraceMotion {
    /// `FADE_MS`, `GRACE_MS`, `OPACITY`, `CROSS_MM`, `TIP_EASE`: braceghost.js:11-17.
    static let fadeMS = 120.0
    static let graceMS = 150.0
    static let opacity: Float = 0.45
    static let crossMM = 4.0
    static let tipEase: CGFloat = 0.35
    /// `FADED` and `FADE_MS` of ringfade.js: the share of their usual opacity the rings keep while a brace is aimed.
    static let ringsFaded: Float = 0.15

    struct Ghost {
        var mesh: OverlayMesh
        /// `on`: fading in; otherwise out and away.
        var on: Bool
        var opacity: Float
    }

    /// The label (`#brace-tip`).
    struct Tip: Equatable {
        var text = ""
        var shown = false
        var bad = false
        /// Where it sits in the view, in points; it eases there.
        var at: CGPoint?
    }

    private(set) var ghosts: [Ghost] = []
    private(set) var tip = Tip()
    /// The cursor dot's colour, eased between `DrawPalette.cursor` and `DrawPalette.cursorBad`.
    private(set) var cursorColor = DrawPalette.cursor
    /// 1 = rings fully shown.
    private(set) var ringFade: Float = 1

    @ObservationIgnored private var height: Double?
    @ObservationIgnored private var cursorBad = false
    @ObservationIgnored private var tipTo: CGPoint?
    @ObservationIgnored private var ringTarget: Float = 1
    /// A grace is running: the pointer left the side, the preview waits `graceMS` before it fades.
    @ObservationIgnored private var graceLeft: Double?
    @ObservationIgnored private var graceDone: (() -> Void)?
    @ObservationIgnored private var loop: Task<Void, Never>?
    @ObservationIgnored private var last: ContinuousClock.Instant?
    /// Something changed on screen.
    @ObservationIgnored var onChange: (() -> Void)?
    /// The rings' fade changed.
    @ObservationIgnored var onRings: ((Float) -> Void)?
    /// False in a test: no frame loop, `advance(ms:)` is stepped by hand.
    @ObservationIgnored var runsLoop = true

    var isEmpty: Bool { ghosts.isEmpty }
    var graceActive: Bool { graceLeft != nil }

    /// `showBrace`: show the brace `mesh` (height `h` mm); nil `mesh` = this spot is refused.
    func show(_ mesh: OverlayMesh?, height h: Double, text: String, bad: Bool) {
        cancelGrace()
        let cur = ghosts.firstIndex { $0.on }
        if let mesh {
            if let cur, let height, abs(h - height) <= Self.crossMM {
                // a step: same ghost, new shape, opacity untouched
                ghosts[cur].mesh = mesh
            } else {
                if let cur { ghosts[cur].on = false }
                ghosts.append(Ghost(mesh: mesh, on: true, opacity: 0))
            }
            height = h
        } else {
            if let cur { ghosts[cur].on = false }
            height = nil
        }
        cursorBad = bad
        fadeRings(true)
        tip.text = text
        tip.bad = bad
        tip.shown = true
        kick()
        onChange?()
    }

    /// `moveTip`: where the label should sit; it eases there, or jumps if hidden.
    func moveTip(to p: CGPoint) {
        tipTo = p
        if !tip.shown || tip.at == nil { tip.at = p }
        kick()
    }

    /// `hideBrace`: fade the preview out, after `graceMS` when `soft` (the pointer may come straight back). `then`
    /// runs when the fade starts, e.g. to hide the cursor dot; a later soft call inside the grace replaces it (the
    /// latest place the pointer went decides).
    func hide(soft: Bool, then: (() -> Void)? = nil) {
        if soft {
            graceDone = then
            if graceLeft != nil { return }
            graceLeft = Self.graceMS
            kick()
            return
        }
        finishGrace()
        fadeAll()
        then?()
    }

    /// `dropBrace`: gone this frame (the geometry is stale after a rebuild, or a click just placed it).
    func drop() {
        finishGrace()
        ghosts = []
        height = nil
        cursorBad = false
        fadeRings(false)
        cursorColor = DrawPalette.cursor
        tip.shown = false
        tip.bad = false
        tip.at = nil
        tipTo = nil
        onChange?()
    }

    private func cancelGrace() {
        graceLeft = nil
        graceDone = nil
    }

    /// End a grace early, still doing what it was going to (e.g. hide the dot).
    private func finishGrace() {
        let f = graceDone
        cancelGrace()
        f?()
    }

    private func fadeAll() {
        for i in ghosts.indices { ghosts[i].on = false }
        fadeRings(false)
        height = nil
        cursorBad = false
        tip.shown = false
        tip.at = nil
        tipTo = nil
        kick()
        onChange?()
    }

    private func fadeRings(_ back: Bool) {
        ringTarget = back ? Self.ringsFaded : 1
        kick()
    }

    // MARK: stepping

    /// One frame: step the grace, the fades, the label and the cursor colour. True while something is still moving.
    @discardableResult
    func advance(ms dt: Double) -> Bool {
        var busy = false
        if let g = graceLeft {
            let left = g - dt
            if left > 0 { graceLeft = left; busy = true } else {
                let f = graceDone
                cancelGrace()
                fadeAll()
                f?()
            }
        }
        let step = Float(min(1, max(0, dt) / Self.fadeMS)) * Self.opacity
        for i in ghosts.indices {
            let to: Float = ghosts[i].on ? Self.opacity : 0
            let o = ghosts[i].opacity
            ghosts[i].opacity = to > o ? min(to, o + step) : max(to, o - step)
            if ghosts[i].opacity != to { busy = true }
        }
        ghosts.removeAll { !$0.on && $0.opacity <= 0 }
        if let at = tip.at, let to = tipTo {
            let dx = to.x - at.x, dy = to.y - at.y
            if abs(dx) + abs(dy) > 0.5 {
                tip.at = CGPoint(x: at.x + dx * Self.tipEase, y: at.y + dy * Self.tipEase)
                busy = true
            } else { tip.at = to }
        }
        let to = cursorBad ? DrawPalette.cursorBad : DrawPalette.cursor
        let c = cursorColor
        if abs(c.x - to.x) + abs(c.y - to.y) + abs(c.z - to.z) > 0.01 {
            cursorColor = c + (to - c) * min(1, step / Self.opacity)
            busy = true
        } else { cursorColor = to }
        // the rings (ringfade.js): FADE_MS to cross the faded share
        let rstep = Float(max(0, dt) / Self.fadeMS) * (1 - Self.ringsFaded)
        let r = ringFade
        let newRing = ringTarget > r ? min(ringTarget, r + rstep) : max(ringTarget, r - rstep)
        if newRing != r { ringFade = newRing; onRings?(newRing) }
        if ringFade != ringTarget { busy = true }
        onChange?()
        return busy
    }

    /// The window closed: the frame loop ends.
    func stop() { loop?.cancel(); loop = nil }

    /// The frame loop: runs while something moves.
    private func kick() {
        guard runsLoop, loop == nil else { return }
        last = ContinuousClock.now
        loop = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(8))
                guard let self else { return }
                let now = ContinuousClock.now
                let dt = Double((now - (self.last ?? now)) / .milliseconds(1))
                self.last = now
                if !self.advance(ms: dt) { self.loop = nil; return }
            }
        }
    }
}
