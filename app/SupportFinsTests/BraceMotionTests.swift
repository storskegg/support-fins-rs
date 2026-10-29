import CoreGraphics
import Metal
import Testing
import Viewer
import simd

@testable import Support_Fins

/// The sway brace preview's motion against `web/ui/braceghost.js`: stepped by hand, so the numbers are the web's
/// (`FADE_MS` 120, `GRACE_MS` 150, opacity 0.45, a height jump past 4 mm cross-fades, the label eases 0.35 a frame).
@MainActor
@Suite("Brace motion")
struct BraceMotionTests {
    private func mesh() -> OverlayMesh {
        OverlayMesh(buffer: MTLCreateSystemDefaultDevice()!.makeBuffer(length: 256, options: .storageModeShared)!, vertices: 3)
    }

    private func motion() -> BraceMotion {
        let m = BraceMotion()
        m.runsLoop = false
        return m
    }

    @Test func aGhostFadesInOverOneFadeTimeAndStopsAtTheWallGhostsOpacity() {
        let m = motion()
        m.show(mesh(), height: 40, text: "Brace · 40 mm · move up for full height", bad: false)
        #expect(m.ghosts.count == 1 && m.ghosts[0].opacity == 0 && m.ghosts[0].on)
        m.advance(ms: 60)
        #expect(abs(m.ghosts[0].opacity - 0.225) < 1e-6)
        m.advance(ms: 60)
        #expect(abs(m.ghosts[0].opacity - 0.45) < 1e-6)
        #expect(!m.advance(ms: 16) || m.tip.at != nil, "settled, or the label is still easing")
        #expect(m.ghosts[0].opacity == 0.45)
        #expect(m.tip.text == "Brace · 40 mm · move up for full height" && m.tip.shown && !m.tip.bad)
    }

    @Test func aSmallStepChangesTheShapeInPlaceAndABigJumpCrossFades() {
        let m = motion()
        m.show(mesh(), height: 40, text: "a", bad: false)
        m.advance(ms: 120)
        m.show(mesh(), height: 43, text: "b", bad: false)         // within 4 mm: same ghost, opacity untouched
        #expect(m.ghosts.count == 1 && m.ghosts[0].opacity == 0.45)
        m.show(mesh(), height: 60, text: "c", bad: false)         // a jump: the old one fades out, a new one in
        #expect(m.ghosts.count == 2 && !m.ghosts[0].on && m.ghosts[1].on && m.ghosts[1].opacity == 0)
        m.advance(ms: 120)
        #expect(m.ghosts.count == 1 && m.ghosts[0].on, "the old ghost is gone once faded out")
    }

    @Test func aRefusedSpotFadesTheGhostOutAndTurnsTheCursorAmber() {
        let m = motion()
        m.show(mesh(), height: 40, text: "ok", bad: false)
        m.advance(ms: 120)
        m.show(nil, height: 0, text: "that’s too low for a brace — move up the side", bad: true)
        #expect(!m.ghosts[0].on && m.tip.bad)
        for _ in 0..<40 { m.advance(ms: 16) }
        #expect(m.ghosts.isEmpty)
        #expect(m.cursorColor == DrawPalette.cursorBad, "eased all the way to amber")
        m.show(mesh(), height: 40, text: "ok", bad: false)
        for _ in 0..<40 { m.advance(ms: 16) }
        #expect(m.cursorColor == DrawPalette.cursor)
    }

    @Test func aSoftHideWaitsOutTheGraceAndAnotherShowCancelsIt() {
        let m = motion()
        var hidden = 0
        m.show(mesh(), height: 40, text: "a", bad: false)
        m.advance(ms: 120)
        m.hide(soft: true) { hidden += 1 }
        #expect(m.graceActive)
        m.advance(ms: 100)
        #expect(m.ghosts[0].on && hidden == 0, "still inside the 150 ms grace")
        m.show(mesh(), height: 41, text: "back on the side", bad: false)   // the pointer came straight back
        #expect(!m.graceActive)
        m.advance(ms: 400)
        #expect(m.ghosts.count == 1 && m.ghosts[0].on && hidden == 0)
        m.hide(soft: true) { hidden += 1 }
        m.advance(ms: 100)
        m.advance(ms: 60)                       // past 150 ms: the fade starts and the callback runs
        #expect(hidden == 1 && !m.ghosts[0].on && !m.tip.shown)
    }

    @Test func aHardHideStartsTheFadeAtOnceAndFinishesAGraceBeforeIt() {
        let m = motion()
        var ran: [String] = []
        m.show(mesh(), height: 40, text: "a", bad: false)
        m.hide(soft: true) { ran.append("soft") }
        m.hide(soft: false) { ran.append("hard") }
        #expect(ran == ["soft", "hard"] && !m.graceActive && !m.ghosts[0].on)
    }

    @Test func theLabelEasesAfterTheCursorAndJumpsWhenHidden() {
        let m = motion()
        m.moveTip(to: CGPoint(x: 100, y: 100))
        #expect(m.tip.at == CGPoint(x: 100, y: 100), "hidden: it jumps")
        m.show(mesh(), height: 10, text: "a", bad: false)
        m.moveTip(to: CGPoint(x: 200, y: 100))
        m.advance(ms: 16)
        #expect(abs(m.tip.at!.x - (100 + 100 * 0.35)) < 1e-9, "35% of the gap a frame")
        for _ in 0..<60 { m.advance(ms: 16) }
        #expect(m.tip.at == CGPoint(x: 200, y: 100))
    }

    @Test func theRingsFadeBackWhileABraceIsAimedAndComeBack() {
        let m = motion()
        var seen: [Float] = []
        m.onRings = { seen.append($0) }
        m.show(mesh(), height: 10, text: "a", bad: false)
        for _ in 0..<20 { m.advance(ms: 16) }
        #expect(m.ringFade == BraceMotion.ringsFaded && seen.first! < 1)
        m.drop()
        for _ in 0..<20 { m.advance(ms: 16) }
        #expect(m.ringFade == 1)
        #expect(m.tip.shown == false && m.ghosts.isEmpty && m.cursorColor == DrawPalette.cursor)
    }
}
