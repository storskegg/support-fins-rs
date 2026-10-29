import Testing

@testable import Support_Fins

/// `tests/rest.test.js`: the Sway brace ghost is built once the cursor rests, not per move.
@MainActor
@Suite("Rest")
struct RestTests {
    private func sleep(_ ms: Int) async { try? await Task.sleep(for: .milliseconds(ms)) }

    @Test func theWebsRestIsEightyMilliseconds() {
        #expect(Rest<Int>.restMS == 80)
    }

    @Test func aBurstOfMovesRunsOnceWithTheLastTargetAfterThePause() async {
        var seen: [Int] = []
        let r = Rest<Int>(ms: 60) { seen.append($0) }
        for i in 0..<6 {
            r.poke(i)
            await sleep(8)                            // faster than the pause: nothing yet
        }
        #expect(seen.isEmpty && r.pending, "ran while the cursor was still moving")
        await sleep(150)
        #expect(seen == [5], "want one run on the last target, got \(seen)")
        #expect(!r.pending, "still pending after it ran")
    }

    @Test func cancelDropsThePendingRun() async {
        var seen: [String] = []
        let r = Rest<String>(ms: 40) { seen.append($0) }
        r.poke("a")
        r.cancel()
        await sleep(100)
        #expect(seen.isEmpty && !r.pending, "a cancelled run still fired")
        r.poke("b")
        await sleep(100)
        #expect(seen == ["b"], "poke after cancel should run again")
    }
}
