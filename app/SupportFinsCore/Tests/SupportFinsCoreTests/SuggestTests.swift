import Foundation
import Testing

@testable import SupportFinsCore

private func modelPath(_ name: String) throws -> String {
    var url = URL(fileURLWithPath: #filePath)
    for _ in 0..<5 { url.deleteLastPathComponent() }
    let upstream = try String(contentsOf: url.appendingPathComponent("UPSTREAM"), encoding: .utf8)
    let line = try #require(upstream.split(separator: "\n").first { $0.hasPrefix("checkout=") })
    var root = String(line.dropFirst("checkout=".count)).trimmingCharacters(in: .whitespaces)
    if root.hasPrefix("~") { root = NSHomeDirectory() + root.dropFirst() }
    return root + "/prototype/stress/models/\(name).stl"
}

private final class Collector: SuggestListener, @unchecked Sendable {
    enum Event: Sendable {
        case suggested(UInt64, SuggestResult)
        case progress(UInt64, Double)
        case strength(UInt64, StrengthAdvice)
        case failed(UInt64, FfiError)
    }
    let stream: AsyncStream<Event>
    private let continuation: AsyncStream<Event>.Continuation
    init() { (stream, continuation) = AsyncStream.makeStream() }
    func suggested(generation: UInt64, result: SuggestResult) { continuation.yield(.suggested(generation, result)) }
    func progress(generation: UInt64, fraction: Double) { continuation.yield(.progress(generation, fraction)) }
    func strength(generation: UInt64, advice: StrengthAdvice) { continuation.yield(.strength(generation, advice)) }
    func failed(generation: UInt64, err: FfiError) { continuation.yield(.failed(generation, err)) }
    func finish() { continuation.finish() }
}

private let context = SuggestContext(currentBore: 0, density: 1.24)

@Test func theSuggestPopoverRowsComeFromRust() async throws {
    let part = try #require(try loadParts(path: try modelPath("cube")).first)
    let suggester = Suggester(part: part)
    let collector = Collector()
    let generation = suggester.suggest(thresholdDeg: 45, optsJson: "{}", context: context, listener: collector)
    for await event in collector.stream {
        if case .progress = event { continue }  // the button's percentage; the ranking comes last
        guard case .suggested(let g, let result) = event else { Issue.record("not a suggestion"); return }
        #expect(g == generation)
        let best = try #require(result.rows.first)
        #expect(best.rank == "Best" && best.line == "40 mm · no fins")
        #expect(best.tier == .free && best.badge == nil && !best.cantPrint)
        #expect(best.rot.count == 9)
        #expect(result.noteGood && result.note.hasPrefix("This way up it needs no fins, 0 g."))
        // the same ranking, blocking, equals the delivered one
        #expect(try part.suggestOrientations(thresholdDeg: 45, optsJson: "{}", context: context) == result)
        break
    }
}

/// `onProgress` of `suggestOrientations` (suggest_worker.test.js:47): after every pose, climbing to 1 before the
/// ranking arrives, for the web's search and the wide one.
@Test func aRankingReportsProgressAfterEveryPoseClimbingToOne() async throws {
    let part = try #require(try loadParts(path: try modelPath("torus")).first)
    for density in [nil, 10.0] as [Double?] {
        let suggester = Suggester(part: part)
        let collector = Collector()
        let generation = density.map {
            suggester.suggestWide(thresholdDeg: 45, optsJson: "{}", context: context, density: $0, listener: collector)
        } ?? suggester.suggest(thresholdDeg: 45, optsJson: "{}", context: context, listener: collector)
        var seen: [Double] = []
        for await event in collector.stream {
            switch event {
            case .progress(let g, let f):
                #expect(g == generation, "progress without its generation")
                seen.append(f)
            case .suggested(let g, let result):
                #expect(g == generation && !result.rows.isEmpty)
                collector.finish()
            default: Issue.record("unexpected event")
            }
        }
        #expect(seen.count >= 6, "only \(seen.count) progress messages")
        #expect(zip(seen, seen.dropFirst()).allSatisfy { $0 < $1 }, "progress went backwards: \(seen)")
        #expect(abs((seen.last ?? 0) - 1) < 1e-9)
    }
}

@Test func aSupersededRankingIsNeverDelivered() async throws {
    let part = try #require(try loadParts(path: try modelPath("sphere")).first)
    let suggester = Suggester(part: part)
    let first = Collector(), second = Collector()
    _ = suggester.suggest(thresholdDeg: 45, optsJson: "{}", context: context, listener: first)
    let g2 = suggester.suggest(thresholdDeg: 45, optsJson: "{}", context: context, listener: second)
    for await event in second.stream {
        if case .progress = event { continue }  // the button's percentage; the ranking comes last
        guard case .suggested(let g, _) = event else { Issue.record("not a suggestion"); return }
        #expect(g == g2)
        break
    }
    try await Task.sleep(for: .milliseconds(300))
    first.finish()
    var stale = 0
    for await _ in first.stream { stale += 1 }
    #expect(stale == 0)
}

@Test func theWideSearchRanksTheSameWayOnMoreCandidates() async throws {
    let part = try #require(try loadParts(path: try modelPath("sphere")).first)
    // density 1 is the web's own candidate set
    let web = try part.suggestOrientations(thresholdDeg: 45, optsJson: "{}", context: context)
    #expect(try part.suggestOrientationsWide(thresholdDeg: 45, optsJson: "{}", context: context, density: 1) == web)
    let suggester = Suggester(part: part)
    let collector = Collector()
    let generation = suggester.suggestWide(thresholdDeg: 45, optsJson: "{}", context: context, density: 10, listener: collector)
    for await event in collector.stream {
        if case .progress = event { continue }  // the button's percentage; the ranking comes last
        guard case .suggested(let g, let result) = event else { Issue.record("not a suggestion"); return }
        #expect(g == generation)
        #expect(result.rows.count == 3 && result.rows[0].rank == "Best")
        break
    }
}

@Test func loadAlignmentGivesTheVerdictTheStrengthPopoverShows() throws {
    let up = try #require(try loadAlignment(dirWorld: [0, 0, 1]))
    #expect(up.quality == "poor" && up.canSuggest)
    #expect(up.text == "The load pulls straight across the layers — where prints split first.")
    let along = try #require(try loadAlignment(dirWorld: [1, 0, 0]))
    #expect(along.quality == "good" && !along.canSuggest)
    #expect(try loadAlignment(dirWorld: [0, 0, 0]) == nil)
    #expect(throws: FfiError.self) { try loadAlignment(dirWorld: [1]) }
}

@Test func theStrongestPoseAdviceTurnsThePartOrSaysItIsAlreadyStrongest() async throws {
    let part = try #require(try loadParts(path: try modelPath("lbracket")).first)
    let suggester = Suggester(part: part)
    let collector = Collector()
    let g = suggester.strength(dirLocal: [0, 0, 1], dirWorld: [0, 0, 1], thresholdDeg: 45, listener: collector)
    for await event in collector.stream {
        guard case .strength(let got, let advice) = event else { Issue.record("not strength advice"); return }
        #expect(got == g)
        #expect(advice == (try part.suggestStrengthPose(dirLocal: [0, 0, 1], dirWorld: [0, 0, 1], thresholdDeg: 45)))
        if let pose = advice.pose { #expect(pose.rot.count == 9 && !advice.alreadyStrongest) }
        else { #expect(advice.alreadyStrongest && advice.note?.hasPrefix("This is about the strongest") == true) }
        break
    }
}

private final class BuildCollector: BuildListener, @unchecked Sendable {
    let stream: AsyncStream<BuildResult>
    private let continuation: AsyncStream<BuildResult>.Continuation
    init() { (stream, continuation) = AsyncStream.makeStream() }
    func done(generation: UInt64, result: BuildResult) { continuation.yield(result) }
    func failed(generation: UInt64, err: FfiError) { continuation.finish() }
}

@Test func theBuildSummaryCarriesTheReadoutData() async throws {
    let part = try #require(try loadParts(path: try modelPath("sphere")).first)
    let collector = BuildCollector()
    _ = Builder(part: part).build(
        rot: [1, 0, 0, 0, 0.8660254037844387, 0.49999999999999994, 0, -0.49999999999999994, 0.8660254037844387],
        optsJson: "{}", tunablesJson: "{}", listener: collector)
    var built: BuildResult?
    for await result in collector.stream { built = result; break }
    let result = try #require(built)
    let summary = try #require(try JSONSerialization.jsonObject(with: Data(result.summaryJson().utf8)) as? [String: Any])
    #expect(summary["boreWalls"] as? Double == 0)
    #expect((summary["finVolumeMm3"] as? Double ?? 0) > 0)
    let seating = try #require(summary["seating"] as? [String: Any])
    #expect(["face", "edge", "point"].contains(seating["kind"] as? String ?? ""))
    #expect(summary["hasPad"] is Bool)
}

@Test func theReadoutNumbersReachSwift() async throws {
    let part = try #require(try loadParts(path: try modelPath("sphere")).first)
    let rot = [1, 0, 0, 0, 0.8660254037844387, 0.49999999999999994, 0, -0.49999999999999994, 0.8660254037844387]
    let a = try part.analyze(thresholdDeg: 45, rot: rot)
    #expect(a.rawRegionCount >= a.regionCount && a.analyzeMs >= 0)
    #expect(part.info().weldMs > 0)
    _ = try part.islands(thresholdDeg: 45, rot: rot)
    #expect(islandCard(count: 0, bare: nil, capped: false, markersVisible: true) == "")
    #expect(islandCard(count: 2, bare: nil, capped: false, markersVisible: true).hasPrefix("⚠ 2 spots start in midair (red cones)"))

    let collector = BuildCollector()
    _ = Builder(part: part).build(rot: rot, optsJson: "{}", tunablesJson: "{}", listener: collector)
    var built: BuildResult?
    for await result in collector.stream { built = result; break }
    let result = try #require(built)
    let summary = try #require(try JSONSerialization.jsonObject(with: Data(result.summaryJson().utf8)) as? [String: Any])
    #expect(summary["addedBounds"] != nil && summary["fillReach"] != nil)
    let removalText = try result.afterRemovalJson(removedIds: [])
    let removal = try #require(try JSONSerialization.jsonObject(with: Data(removalText.utf8)) as? [String: Any])
    #expect(removal["removed"] as? Double == 0)
}
