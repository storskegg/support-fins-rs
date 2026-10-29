import Foundation
import Testing
import simd

@testable import Support_Fins

/// The walls' JSON for the engine, the sweep's answer, and the settings the engine builds them under.
@Suite("Draw walls")
struct DrawWallTests {
    @Test func aWallIsTheEnginesJSONInThePartsFrame() {
        let w = DrawnWall.wall(from: SIMD3(1, 2, 3.5), to: SIMD3(-4, 0.25, 6))
        #expect(w.json == "{\"kind\":\"wall\",\"a\":[1,2,3.5],\"b\":[-4,0.25,6]}")
        let c = DrawnWall.wall(from: SIMD3(0, 0, 0), to: SIMD3(10, 0, 0), through: SIMD3(5, 2, 0))
        #expect(c.json == "{\"kind\":\"wall\",\"a\":[0,0,0],\"b\":[10,0,0],\"m\":[5,2,0]}")
        let b = DrawnWall.brace(at: SIMD3(7, -3, 12), face: 42, capped: true)
        #expect(b.json == "{\"kind\":\"sway\",\"a\":[7,-3,12],\"face\":42,\"capped\":true}")
        #expect([w, b].json == "[\(w.json),\(b.json)]")
    }

    @Test func aSweepsAnswerDecodesWithAbsentKeysAsZero() {
        let outs = WallOutcome.decode("""
        [{"ok":true,"triStart":0,"triEnd":480,"kind":"wall","length":17.24,"height":14.9,"tines":9,"ribs":2},
         {"ok":false,"triStart":480,"triEnd":480,"reason":"wall too short — 0.5mm, needs 2mm"},
         {"ok":true,"triStart":480,"triEnd":700,"kind":"sway","height":299,"tines":50,"cross":2,"stilt":0.6}]
        """)
        #expect(outs.count == 3)
        #expect(outs[0].ok && outs[0].length == 17.24 && outs[0].tines == 9 && outs[0].ribs == 2 && outs[0].cross == 0)
        #expect(!outs[1].ok && outs[1].reason == "wall too short — 0.5mm, needs 2mm" && outs[1].tines == 0)
        #expect(outs[2].kind == "sway" && outs[2].cross == 2 && outs[2].stilt == 0.6)
        #expect(WallOutcome.decode("not json").isEmpty)
    }

    @Test func theSettingsAreThePagesControlsDividedAsThePageDividesThem() {
        var s = FinSettings()
        s.tineDensity = 60
        s.swayDepth = 15
        s.swayCrossHeight = 95          // the page clamps it to 10...90
        s.swayFrom = 20
        s.fullBraceDepth = true
        s.plateOnly = true
        let o = (try? JSONSerialization.jsonObject(with: Data(s.drawSettingsJSON().utf8))) as? [String: Any]
        #expect(o?["tines"] as? Bool == true && o?["plateOnly"] as? Bool == true && o?["tallRibs"] as? Bool == true)
        #expect(o?["tineDensity"] as? Double == 60.0 / 100)
        #expect(o?["layerHeight"] as? Double == 0.2)
        let sway = o?["sway"] as? [String: Any]
        #expect(sway?["reach"] as? Double == 15.0 / 100 && sway?["crossRise"] as? Double == 0.9)
        #expect(sway?["gripFrom"] as? Double == 20 && sway?["tineSpacing"] as? Double == 6 && sway?["uncappedDepth"] as? Bool == true)
    }

    @Test func drawBuildsOnlyThePadAndTheOtherModesKeepTheirs() {
        var s = FinSettings()
        s.mode = .draw
        #expect(s.optionsJSON().contains("\"mode\":\"draw\""))
        #expect(s.optionsJSON().contains("\"sway.on\":false"), "no Auto braces in Draw")
        s.mode = .full
        #expect(s.optionsJSON().contains("\"mode\":\"full\""))
    }
}
