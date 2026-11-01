import Testing

@testable import Support_Fins

/// The Overhang slider keeps whole degrees without SwiftUI's `step:` (whose per-degree tick marks drew a faint second
/// track under it, user suggestion S5): a drag rounds, and a nudge smaller than half a degree (an arrow key, VoiceOver's
/// increment) still moves one degree, so the slider never sticks.
@Suite("Overhang slider")
struct OverhangSliderTests {
    @Test func aDragRoundsToWholeDegrees() {
        #expect(OverhangSlider.snap(47.4, from: 45) == 47)
        #expect(OverhangSlider.snap(47.6, from: 45) == 48)
        #expect(OverhangSlider.snap(30.2, from: 45) == 30)
    }

    @Test func aSmallNudgeMovesOneDegree() {
        #expect(OverhangSlider.snap(45.4, from: 45) == 46)
        #expect(OverhangSlider.snap(44.9, from: 45) == 44)
    }

    @Test func noMoveStays() {
        #expect(OverhangSlider.snap(45, from: 45) == 45)
    }

    @Test func itStaysInTheRange() {
        #expect(OverhangSlider.snap(70.3, from: 70) == 70)
        #expect(OverhangSlider.snap(29.8, from: 30) == 30)
        #expect(OverhangSlider.snap(71, from: 65) == 70)
    }
}
