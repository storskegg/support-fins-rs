import SupportFinsCore
import Testing

@testable import Support_Fins

@MainActor
@Suite("Suggest orientation")
struct SuggestModelTests {
    @Test func rankingRunsOnClickNeverOnCreation() async throws {
        let pose = FakePose()
        let model = SuggestModel(part: try TestParts.cube(), pose: pose.access)
        #expect(model.state == .idle && model.rows.isEmpty && model.buttonTitle == "Suggest orientation")
        model.rank(thresholdDegrees: 45, optionsJSON: "{}", currentBore: 0, density: 1.24)
        #expect(model.buttonTitle == "Ranking…" && model.isRanking)
        #expect(await eventually { !model.isRanking })
        guard case .ready = model.state else { Issue.record("ranking did not finish: \(model.state)"); return }
        #expect(model.buttonTitle == "Suggest orientation")
    }

    @Test func aPartThatNeedsNoFinsGetsTheGreenRowAndTheWinAsTheNote() async throws {
        let model = SuggestModel(part: try TestParts.cube(), pose: FakePose().access)
        model.rank(thresholdDegrees: 45, optionsJSON: "{}", currentBore: 0, density: 1.24)
        #expect(await eventually { !model.isRanking })
        let best = try #require(model.rows.first)
        // the page's words: "Best", then "{height} mm · no fins"
        #expect(best.rank == "Best" && best.line == "40 mm · no fins")
        #expect(best.tier == .free && best.badge == nil && !best.cantPrint)
        #expect(model.noteIsGood && model.note.hasPrefix("This way up it needs no fins, 0 g."))
        #expect(model.rows.dropFirst().enumerated().allSatisfy { $0.element.rank == "#\($0.offset + 2)" })
    }

    @Test func aRowsLineIsTheEnginesHeightAndFinCount() async throws {
        let model = SuggestModel(part: try TestParts.tShape(), pose: FakePose().access)
        model.rank(thresholdDegrees: 45, optionsJSON: "{}", currentBore: 0, density: 1.24)
        #expect(await eventually { !model.isRanking })
        #expect(!model.rows.isEmpty)
        for r in model.rows {
            let fins = r.fins == 0 ? "no fins" : r.fins == 1 ? "1 fin" : "\(r.fins) fins"
            #expect(r.line.hasPrefix("\(Int(r.height.rounded())) mm · \(fins)"), "\(r.line)")
        }
        if model.rows[0].tier == .ordinary { #expect(model.note == "Click a pose to turn the part." && !model.noteIsGood) }
    }

    @Test func aClickOnARowTurnsThePartOnceAndAManualTurnDropsTheHighlight() async throws {
        let pose = FakePose()
        let model = SuggestModel(part: try TestParts.tShape(), pose: pose.access)
        model.rank(thresholdDegrees: 45, optionsJSON: "{}", currentBore: 0, density: 1.24)
        #expect(await eventually { !model.isRanking })
        let row = try #require(model.rows.last)
        model.pick(model.rows.count - 1)
        #expect(pose.applied == [row.rot] && model.picked == model.rows.count - 1)
        // the turn the click made keeps the highlight; a ring drag does not
        model.poseChanged()
        #expect(model.picked == model.rows.count - 1)
        pose.rotation = eulerMatrix(10, 0, 0)
        model.poseChanged()
        #expect(model.picked == nil)
        // the list stays, and any row can still be clicked
        model.pick(0)
        #expect(pose.applied.count == 2)
    }

    @Test func aRankingThatIsResetIsNeverShown() async throws {
        let model = SuggestModel(part: try TestParts.tShape(), pose: FakePose().access)
        model.rank(thresholdDegrees: 45, optionsJSON: "{}", currentBore: 0, density: 1.24)
        model.reset()
        #expect(model.state == .idle)
        try await Task.sleep(for: .seconds(1))
        #expect(model.state == .idle && model.rows.isEmpty, "the answer to a dropped ranking never lands")
    }

    @Test func badOptionsAreShownAsTheEnginesMessage() async throws {
        let model = SuggestModel(part: try TestParts.cube(), pose: FakePose().access)
        model.rank(thresholdDegrees: 45, optionsJSON: #"{"mode":"bogus"}"#, currentBore: 0, density: 1.24)
        #expect(await eventually { !model.isRanking })
        guard case .failed(let message) = model.state else { Issue.record("expected a failure: \(model.state)"); return }
        #expect(!message.isEmpty && model.note == message)
    }

    @Test func withoutAPartThereIsNothingToRank() {
        let model = SuggestModel(part: nil, pose: FakePose().access)
        model.rank(thresholdDegrees: 45, optionsJSON: "{}", currentBore: 0, density: 1.24)
        #expect(model.state == .idle)
    }
}
