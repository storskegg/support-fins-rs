import Foundation
import Testing

@testable import Support_Fins

/// How long the main thread stalls while plate ties are turned on (issue #50: detection and the ties run off the main
/// actor; only the request and the drawing are on it). A measurement, not a check of a fixture: it runs only when a
/// multi-object 3MF has been copied into the test host's temporary directory as `sf-measure/plate.3mf` (the host is
/// sandboxed, so `~/Library/Containers/<app id>/Data/tmp/sf-measure/plate.3mf`), and prints what it saw. Each object
/// goes onto one plate, in a row along its thin side 4 mm apart, fins off.
@MainActor
@Suite("Plate ties: main-thread stalls")
struct PlateTiesStallTests {
    nonisolated static let file = URL(fileURLWithPath: NSTemporaryDirectory()).appending(path: "sf-measure/plate.3mf")

    @Test(.enabled(if: FileManager.default.fileExists(atPath: file.path)), .timeLimit(.minutes(20)))
    func turningTiesOnKeepsTheMainThreadFree() async throws {
        let model = ViewerModel()
        try #require(model.controller != nil, "no Metal device")
        await model.load(url: Self.file)
        let first = try #require(model.picker, "a file of several objects")
        let entries = first.entries.sorted { $0.id < $1.id }
        var sizes: [[Int]] = []
        for (k, e) in entries.enumerated() {
            if k > 0 {
                await model.load(url: Self.file, adding: true)
            }
            let picker = try #require(model.picker)
            picker.onLoad([e.id])
            // the pick is shown in a Task of its own: wait for the model to be on the plate
            #expect(await eventually(120) { model.picker == nil && !model.isLoading && model.features[PlatesFeature.self]?.board.modelCount == k + 1 })
            sizes.append(e.size)
        }
        let board = try #require(model.features[PlatesFeature.self]?.board)
        let ties = try #require(model.features[PlateTiesFeature.self]?.model)
        try #require(board.plate.models.count == entries.count)
        // a row along each model's thin side, 4 mm between them
        var at = 0.0
        for (k, m) in board.plate.models.enumerated() {
            let s = sizes[k], alongX = s[0] <= s[1]
            let thin = Double(alongX ? s[0] : s[1])
            let xy = alongX ? SIMD2(at + thin / 2, 0) : SIMD2(0, at + thin / 2)
            at += thin + 4
            if k == board.plate.live { board.placeLive(m, xy) } else { m.xy = xy }
        }
        board.sync()

        // a heartbeat on the main actor: the longest gap between its turns is the longest stall
        final class Heart { var beats: [ContinuousClock.Instant] = []; var running = true }
        let h = Heart()
        let heart = Task { @MainActor in
            while h.running {
                h.beats.append(.now)
                try? await Task.sleep(for: .milliseconds(1))
            }
        }
        try await Task.sleep(for: .milliseconds(50))
        let clock = ContinuousClock()
        let switchOn = clock.measure { ties.setOn(true) }
        let start = ContinuousClock.now
        #expect(await ties.settled(seconds: 600))
        let landed = ContinuousClock.now - start
        try await Task.sleep(for: .milliseconds(50))
        h.running = false
        await heart.value
        let gaps = zip(h.beats.dropFirst(), h.beats).map { $0 - $1 }
        let worst = gaps.max() ?? .zero
        let over16 = gaps.filter { $0 > .milliseconds(16) }.count
        let ms = { (d: Duration) in String(format: "%.2f", Double(d.components.attoseconds) / 1e15 + Double(d.components.seconds) * 1000) }
        print("STALL models=\(board.plate.models.count) triangles=\(model.session.parts.first?.faceCount ?? 0)/model pairs=\(ties.links.count) "
              + "ties=\(ties.placed)/\(ties.requested) tieTriangles=\(ties.triangles) setOn=\(ms(switchOn))ms landed=\(ms(landed))ms "
              + "beats=\(gaps.count) worstGap=\(ms(worst))ms gapsOver16ms=\(over16) error=\(ties.error ?? "-")")
        #expect(ties.error == nil)
    }
}
