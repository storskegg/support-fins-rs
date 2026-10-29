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

    @Test func theButtonShowsTheRankingsProgressAsAPercentage() async throws {
        let model = SuggestModel(part: try TestParts.tShape(), pose: FakePose().access)
        model.rank(thresholdDegrees: 45, optionsJSON: "{}", currentBore: 0, density: 1.24)
        let g = try #require(model.wanted)
        // nothing reported yet: the bare label
        #expect(model.buttonTitle == "Ranking…")
        model.deliver(.progress(g, 0.4))
        #expect(model.buttonTitle == "Ranking… 40%")
        // the engine's threads can land out of order: it only rises
        model.deliver(.progress(g, 0.3))
        #expect(model.buttonTitle == "Ranking… 40%")
        // Math.round: a half rounds up
        model.deliver(.progress(g, 0.625))
        #expect(model.buttonTitle == "Ranking… 63%")
        // a superseded ranking's progress is dropped
        model.deliver(.progress(g + 1, 0.99))
        #expect(model.buttonTitle == "Ranking… 63%")
        // a new ranking starts again from the bare label
        model.reset()
        #expect(model.buttonTitle == "Suggest orientation" && model.progress == nil)
        model.deliver(.progress(g, 0.9))
        #expect(model.buttonTitle == "Suggest orientation", "progress with no ranking in flight is ignored")
    }

    @Test func theAppRunsTheWideSearch() async throws {
        #expect(SuggestModel.searchDensity == 10, "ten times the web's candidate poses")
        let model = SuggestModel(part: try TestParts.tShape(), pose: FakePose().access)
        model.rank(thresholdDegrees: 45, optionsJSON: "{}", currentBore: 0, density: 1.24)
        #expect(await eventually { !model.isRanking })
        guard case .ready = model.state else { Issue.record("the wide ranking did not finish: \(model.state)"); return }
        #expect(!model.rows.isEmpty)
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
            let n = try #require(r.fins)
            let fins = n == 0 ? "no fins" : n == 1 ? "1 fin" : "\(n) fins"
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

    /// The toolbar button sends the build's options (`FinSettings().optionsJSON()`), page extras and all
    /// (`tallRibs`, `curvedWalls`, Draw's mode, the sway extras): the ranking must take them, not fail with
    /// "unknown dialog option tallRibs" and show no rows (#206).
    @Test(arguments: [FinSettings.Mode.auto, .full, .draw], [false, true])
    func theAppsOwnOptionsRank(mode: FinSettings.Mode, sway: Bool) async throws {
        var settings = FinSettings()
        settings.mode = mode
        settings.sway = sway
        let model = SuggestModel(part: try TestParts.tShape(), pose: FakePose().access)
        model.rank(thresholdDegrees: 45, optionsJSON: settings.optionsJSON(), currentBore: 0, density: 1.24)
        #expect(await eventually { !model.isRanking })
        guard case .ready = model.state else { Issue.record("ranking failed: \(model.state)"); return }
        #expect(!model.rows.isEmpty)
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
