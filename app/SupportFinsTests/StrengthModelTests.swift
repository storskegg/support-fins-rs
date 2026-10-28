import SupportFinsCore
import Testing
import simd

@testable import Support_Fins

@MainActor
@Suite("Strength arrow")
struct StrengthModelTests {
    @Test func theSixPadButtonsAreTheWorldDirectionsOfThePage() {
        // PAD_DIRS, ui/strength.js:177-181
        let want: [StrengthModel.Pad: SIMD3<Double>] = [
            .up: [0, 0, 1], .down: [0, 0, -1], .right: [1, 0, 0], .left: [-1, 0, 0], .back: [0, 1, 0], .front: [0, -1, 0],
        ]
        for p in StrengthModel.Pad.allCases { #expect(p.world == want[p]) }
        #expect(StrengthModel.Pad.front.label == "⊙ front" && StrengthModel.Pad.back.label == "⊗ back")
        #expect(StrengthModel.Pad.front.help == "Load points toward you")
    }

    @Test func aPadInTheIdentityPoseIsTheLocalDirectionAndLightsTheBadge() throws {
        let model = StrengthModel(part: nil, pose: FakePose().access)
        var told: [SIMD3<Double>?] = []
        model.loadChanged = { told.append($0) }
        #expect(!model.isLit && model.verdict == nil)
        model.setLoad(.up)
        #expect(model.loadLocal == [0, 0, 1] && model.isLit && model.activePad == .up)
        #expect(told == [[0, 0, 1]])
        // straight up across the layers: the weak direction, so the strongest pose is offered
        #expect(model.verdict?.quality == "poor" && model.canSuggest)
        #expect(model.note == "The load pulls straight across the layers — where prints split first.")
        model.setLoad(.right)
        #expect(model.verdict?.quality == "good" && !model.canSuggest && model.activePad == .right)
        model.clear()
        #expect(!model.isLit && model.verdict == nil && model.activePad == nil && told.last! == nil)
    }

    @Test func theLoadIsStoredInThePartsFrameSoItTurnsWithThePart() throws {
        let pose = FakePose()
        pose.rotation = eulerMatrix(90, 0, 0)         // standing the part on its side
        let model = StrengthModel(part: nil, pose: pose.access)
        model.setLoad(.up)
        let local = try #require(model.loadLocal)
        #expect(simd_length(Rot.apply(pose.rotation, local) - [0, 0, 1]) < 1e-12, "world up in this pose")
        #expect(abs(local.z) < 1e-9, "…which is not the part's own up")
        // put the part back: the same local load now points sideways, a different verdict
        pose.rotation = PoseAccess.identity
        model.poseChanged()
        #expect(model.activePad == .back || model.activePad == .front, "now along the other horizontal axis")
        pose.rotation = eulerMatrix(0, 0, 30)
        model.poseChanged()
        #expect(model.activePad == nil, "the highlight clears when the part is turned off the pad's axes")
        pose.rotation = PoseAccess.identity
        model.poseChanged()
        let world = try #require(model.loadWorld)
        #expect(simd_length(world - local) < 1e-12)
        #expect(model.verdict?.quality == "good", "along the layers, as the part now lies")
    }

    @Test func everyPadRoundTripsThroughAnyPose() throws {
        for (x, y, z) in [(0.0, 0.0, 0.0), (30.0, 0.0, 0.0), (45.0, 20.0, -10.0), (-70.0, 33.0, 120.0)] {
            let pose = FakePose()
            pose.rotation = eulerMatrix(x, y, z)
            let model = StrengthModel(part: nil, pose: pose.access)
            for pad in StrengthModel.Pad.allCases {
                model.setLoad(pad)
                let world = try #require(model.loadWorld)
                #expect(simd_length(world - pad.world) < 1e-12, "\(pad) at \(x),\(y),\(z)")
                #expect(model.activePad == pad)
            }
        }
    }

    @Test func turningToTheStrongestPoseAppliesTheEnginesPoseOrSaysItIsAlreadyStrongest() async throws {
        let pose = FakePose()
        let part = try TestParts.tShape()
        let model = StrengthModel(part: part, pose: pose.access)
        model.setLoad(.up)
        #expect(model.canSuggest)
        let local = try #require(model.loadLocal), world = try #require(model.loadWorld)
        let want = try part.suggestStrengthPose(dirLocal: [local.x, local.y, local.z], dirWorld: [world.x, world.y, world.z], thresholdDeg: 45)
        model.turnToStrongest()
        #expect(model.thinking)
        #expect(await eventually { !model.thinking })
        if let p = want.pose {
            #expect(pose.applied == [p.rot] && model.strongestNote == nil)
            #expect(model.verdict != nil, "the verdict follows the new pose")
        } else {
            #expect(pose.applied.isEmpty)
            #expect(model.strongestNote?.hasPrefix("This is about the strongest printable orientation") == true)
            #expect(!model.canSuggest, "the button goes while the note is up")
            // turning the part takes the note down
            model.poseChanged()
            #expect(model.strongestNote == nil)
        }
    }

    @Test func aDroppedAdviceNeverLands() async throws {
        let pose = FakePose()
        let model = StrengthModel(part: try TestParts.tShape(), pose: pose.access)
        model.setLoad(.up)
        model.turnToStrongest()
        model.clear()
        try await Task.sleep(for: .seconds(1))
        #expect(pose.applied.isEmpty && model.strongestNote == nil && !model.isLit)
    }
}
