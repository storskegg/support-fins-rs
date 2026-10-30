import CryptoKit
import Foundation
import SupportFinsCore
import Testing

@testable import Support_Fins

/// Exports run off the main thread (docs/audit-2026-10/concurrency.md C-03): the file appears whole or not at all, Cancel
/// returns at once, and the main thread keeps running during a 1.2M-face 3MF.
@MainActor
@Suite("Export job")
final class ExportJobTests {
    let dir: URL
    init() throws { dir = try Scratch.directory() }
    deinit { Scratch.remove(dir) }

    private func names() -> [String] { ((try? FileManager.default.contentsOfDirectory(atPath: dir.path)) ?? []).sorted() }

    /// A latch the worker waits on until the test lets it go.
    private final class Gate: @unchecked Sendable {
        let started = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        func awaitStart() { started.wait() }
    }

    @Test func theFileIsWrittenToATempFirstAndRenamedWhole() async throws {
        let job = ExportJob()
        let target = dir.appending(path: "a.3mf")
        let gate = Gate()
        let listing = dir
        let seen = LockedBox<[String]>([])
        let task = Task { try await job.run(title: "3MF", to: target) { path in
            try Data("whole".utf8).write(to: URL(fileURLWithPath: path))
            seen.set(((try? FileManager.default.contentsOfDirectory(atPath: listing.path)) ?? []).sorted())
            gate.started.signal()
            gate.release.wait()
        } }
        await Task.detached { gate.awaitStart() }.value
        #expect(job.isRunning && job.title == "3MF")
        #expect(!FileManager.default.fileExists(atPath: target.path), "not at its name while it is being written")
        gate.release.signal()
        #expect(try await task.value)
        #expect(!job.isRunning)
        #expect(seen.get().isEmpty, "no temp beside the target: \(seen.get())")
        #expect(names() == ["a.3mf"], Comment(rawValue: "\(names())"))
        #expect(try Data(contentsOf: target) == Data("whole".utf8))
    }

    @Test func cancelReturnsAtOnceAndLeavesNoFileNorTemp() async throws {
        let job = ExportJob()
        let target = dir.appending(path: "b.3mf")
        let gate = Gate()
        let task = Task { try await job.run(title: "3MF", to: target) { path in
            try Data("half".utf8).write(to: URL(fileURLWithPath: path))
            gate.started.signal()
            gate.release.wait()      // the engine call has no cancel point: it goes on to its end
        } }
        await Task.detached { gate.awaitStart() }.value
        job.cancel()
        #expect(try await task.value == false, "Cancel does not wait for the engine")
        #expect(!job.isRunning)
        gate.release.signal()
        // the worker ends in the background and removes its temp file
        var tries = 0
        while !names().isEmpty, tries < 200 { try await Task.sleep(for: .milliseconds(10)); tries += 1 }
        #expect(names().isEmpty, Comment(rawValue: "\(names())"))
    }

    @Test func aRunAfterACancelIsNotAnsweredByTheCancelledWorker() async throws {
        let job = ExportJob()
        let gate = Gate()
        let first = Task { try await job.run(title: "one", to: dir.appending(path: "one.stl")) { _ in gate.started.signal(); gate.release.wait() } }
        await Task.detached { gate.awaitStart() }.value
        job.cancel()
        _ = try await first.value
        let slow = Gate()
        let second = Task { try await job.run(title: "two", to: dir.appending(path: "two.stl")) { path in
            try Data("2".utf8).write(to: URL(fileURLWithPath: path)); slow.started.signal(); slow.release.wait()
        } }
        await Task.detached { slow.awaitStart() }.value
        gate.release.signal()          // the cancelled worker ends now: the second run goes on
        try await Task.sleep(for: .milliseconds(100))
        #expect(job.isRunning && job.title == "two")
        slow.release.signal()
        #expect(try await second.value)
        #expect(names() == ["two.stl"], Comment(rawValue: "\(names())"))
    }

    @Test func aFailureLeavesNoFileAndKeepsTheOldOne() async throws {
        let job = ExportJob()
        let target = dir.appending(path: "c.stl")
        try Data("old".utf8).write(to: target)
        struct Boom: Error {}
        await #expect(throws: Boom.self) {
            _ = try await job.run(title: "STL", to: target) { path in
                try Data("partial".utf8).write(to: URL(fileURLWithPath: path))
                throw Boom()
            }
        }
        let kept = try Data(contentsOf: target)
        #expect(names() == ["c.stl"] && kept == Data("old".utf8))
        #expect(!job.isRunning)
        // a good run replaces it
        #expect(try await job.run(title: "STL", to: target) { try Data("new".utf8).write(to: URL(fileURLWithPath: $0)) })
        let replaced = try Data(contentsOf: target)
        #expect(names() == ["c.stl"] && replaced == Data("new".utf8))
    }

    /// The sandbox lets a save panel's file be written but not its folder: no temp may be made beside it, and the
    /// finished bytes must still land in the file. A folder with no write permission mimics it (the file itself stays writable).
    @Test func aTargetInAFolderThatCannotBeWrittenIsFilledInPlaceWithNoTempBesideIt() async throws {
        let locked = dir.appending(path: "locked", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: locked, withIntermediateDirectories: true)
        let target = locked.appending(path: "f.3mf")
        try Data("old".utf8).write(to: target)
        let seen = LockedBox<String>("")
        try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: locked.path)
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: locked.path) }
        let job = ExportJob()
        // a failing export leaves the old file as it was
        struct Boom: Error {}
        await #expect(throws: Boom.self) { _ = try await job.run(title: "3MF", to: target) { path in
            try Data("partial".utf8).write(to: URL(fileURLWithPath: path)); throw Boom() } }
        #expect(try Data(contentsOf: target) == Data("old".utf8))
        let ok = try await job.run(title: "3MF", to: target) { path in
            seen.set(path)
            try Data("new".utf8).write(to: URL(fileURLWithPath: path))
        }
        #expect(ok)
        #expect(!seen.get().hasPrefix(locked.path), "the temp is not beside the target")
        #expect(try Data(contentsOf: target) == Data("new".utf8))
        #expect(try FileManager.default.contentsOfDirectory(atPath: locked.path) == ["f.3mf"])
    }

    @Test func aRunWhileOneIsOutIsRefusedByTheModelsAvailability() async throws {
        let part = try TestParts.tShape()
        let model = ExportModel(build: BuildModel(part: part), finsOn: { false }, source: {
            .init(partName: "t.stl", part: part, nozzle: 0.4, layerHeight: 0.2, pose: [1, 0, 0, 0, 1, 0, 0, 0, 1], threshold: 45, bed: [256, 256])
        })
        #expect(model.availability.enabled == [.stl, .threeMF])
        let gate = Gate()
        let task = Task { try await model.job.run(title: "3MF", to: dir.appending(path: "d.3mf")) { path in
            try Data("d".utf8).write(to: URL(fileURLWithPath: path)); gate.started.signal(); gate.release.wait()
        } }
        await Task.detached { gate.awaitStart() }.value
        #expect(model.availability.enabled.isEmpty, "no second export while one writes")
        #expect(await model.write(.stl, to: dir.appending(path: "e.stl")) == nil)
        gate.release.signal()
        _ = try await task.value
        #expect(model.availability.enabled == [.stl, .threeMF])
    }

    // MARK: the main thread and the bytes

    /// A flat torus of `u` x `v` quads (2 u v faces), as a binary STL.
    private func torus(_ u: Int, _ v: Int, name: String) throws -> String {
        var data = Data(count: 80)
        var count = UInt32(2 * u * v)
        withUnsafeBytes(of: &count) { data.append(contentsOf: $0) }
        func p(_ i: Int, _ j: Int) -> [Float] {
            let a = Float(i % u) / Float(u) * 2 * .pi, b = Float(j % v) / Float(v) * 2 * .pi
            let r = 40 + 10 * cos(b)
            return [r * cos(a), r * sin(a), 12 + 10 * sin(b)]
        }
        data.reserveCapacity(84 + 50 * Int(count))
        for i in 0..<u {
            for j in 0..<v {
                for t in [[p(i, j), p(i + 1, j), p(i + 1, j + 1)], [p(i, j), p(i + 1, j + 1), p(i, j + 1)]] {
                    for f in [Float](repeating: 0, count: 3) + t.flatMap({ $0 }) { var x = f; withUnsafeBytes(of: &x) { data.append(contentsOf: $0) } }
                    data.append(contentsOf: [0, 0])
                }
            }
        }
        let path = dir.appending(path: name).path(percentEncoded: false)
        try data.write(to: URL(fileURLWithPath: path))
        return path
    }

    private func sha(_ url: URL) throws -> String {
        SHA256.hash(data: try Data(contentsOf: url)).map { String(format: "%02x", $0) }.joined()
    }

    @Test(.timeLimit(.minutes(3)))
    func aBigThreeMFNeverBlocksTheMainThreadForMoreThan50ms() async throws {
        // 1.2M faces, the audit's torus (docs/audit-2026-10/concurrency.md C-03: 7.4 s on the main actor)
        let part = try #require(try loadParts(path: try torus(1000, 600, name: "stall.stl")).first)
        let model = ExportModel(build: BuildModel(part: part), finsOn: { false }, source: {
            .init(partName: "stall.stl", part: part, nozzle: 0.4, layerHeight: 0.2, pose: [1, 0, 0, 0, 1, 0, 0, 0, 1], threshold: 45, bed: [256, 256])
        })
        // the ticker: a main-actor loop that wants to run every 5 ms; its worst lateness is the worst stall
        let ticks = LockedBox<(max: Double, count: Int)>((0, 0))
        let ticker = Task { @MainActor in
            var last = ContinuousClock.now
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(5))
                let now = ContinuousClock.now
                let gap = (now - last) / .milliseconds(1)
                ticks.set((max(ticks.get().max, gap), ticks.get().count + 1))
                last = now
            }
        }
        let started = ContinuousClock.now
        let url = try #require(await model.write(.threeMF, to: dir.appending(path: "stall.3mf")), "\(model.error ?? "")")
        let took = (ContinuousClock.now - started) / .milliseconds(1)
        ticker.cancel()
        let worst = ticks.get().max
        print("EXPORT-STALL 1.2M-face 3MF: export took \(Int(took)) ms; \(ticks.get().count) main-thread ticks; worst gap \(Int(worst)) ms")
        #expect(ticks.get().count > 20, "the main thread ran during the export")
        #expect(worst < 50 + 5, "worst main-thread stall \(worst) ms")

        // the bytes are the engine's own: the same call made directly (what the main thread used to run)
        let direct = dir.appending(path: "direct.3mf")
        let options = try #require(model.optionsJSON(for: .threeMF))
        let blocked = ContinuousClock.now
        try exportWithDrawn(path: direct.path(percentEncoded: false), parts: [part], builds: [], drawn: nil, optionsJson: options)
        print("EXPORT-STALL the same export called on the main thread (before): blocked \(Int((ContinuousClock.now - blocked) / .milliseconds(1))) ms")
        #expect(try sha(url) == sha(direct), "byte-identical to the synchronous call")
    }
}

/// A value two threads share.
final class LockedBox<T: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var value: T
    init(_ v: T) { value = v }
    func get() -> T { lock.lock(); defer { lock.unlock() }; return value }
    func set(_ v: T) { lock.lock(); value = v; lock.unlock() }

    // MARK: AtomicWrite (audit X-09)

    @Test func atomicWriteReplacesWholeAndAFailedWriteKeepsTheOldFile() throws {
        let target = dir.appending(path: "c.stl")
        try Data("old".utf8).write(to: target)
        try AtomicWrite.write(Data("new contents".utf8), to: target)
        #expect(try Data(contentsOf: target) == Data("new contents".utf8))
        struct Boom: Error {}
        #expect(throws: Boom.self) {
            _ = try AtomicWrite.write(to: target) { path in
                try Data("half".utf8).write(to: URL(fileURLWithPath: path))
                throw Boom()
            }
        }
        #expect(try Data(contentsOf: target) == Data("new contents".utf8), "a failed write leaves the old file")
        #expect(names() == ["c.stl"], Comment(rawValue: "\(names())"))
    }

    @Test func atomicCopyOfAMissingSourceKeepsTheOldFileAndWritesNothingElse() throws {
        let target = dir.appending(path: "d.stl")
        try Data("keep".utf8).write(to: target)
        #expect(throws: (any Error).self) { try AtomicWrite.copy(from: dir.appending(path: "no-such"), to: target) }
        #expect(try Data(contentsOf: target) == Data("keep".utf8))
        #expect(names() == ["d.stl"], Comment(rawValue: "\(names())"))
    }
}
