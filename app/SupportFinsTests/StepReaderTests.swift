import Foundation
import SupportFinsCore
import Testing

@testable import Support_Fins

/// The test bundle's resources: the files that crashed the app (`crates/sf-step-xpc/tests/segv`) and the parity STEP
/// fixtures (`parity/step-fixtures`), as folders (app/project.yml).
private final class BundleToken {}
private let resources = Bundle(for: BundleToken.self).resourceURL!

/// STEP is read in the StepReader XPC service (X-01). These run the real service from the test host (the app): a file
/// that crashes OpenCascade ends the service's process, never this one.
@MainActor
@Suite("STEP reader (X-01)", .serialized)
struct StepReaderTests {
    /// The parts of every parity fixture, as `load_parts` makes them reading in process (crates/sf-ffi): name,
    /// triangles and an FNV-1a hash of the mesh buffer. `crates/sf-ffi/tests/ffi.rs`
    /// (`load_step_parts_makes_what_load_parts_makes`) and `crates/sf-step/tests/parity_step.rs` hold the service's
    /// path to the web's oracle; this holds the app to them through the real service.
    nonisolated static let expected: [String: [(String, UInt64, UInt64)]] = [
        "assembly.step": [("Frame", 12, 0xd1ac7bb6d79e18c5), ("Pair", 1000, 0x8eaab75493a98fdd), ("Lone", 8002, 0x1e1eeae26a538535)],
        "compound-two-solids.step": [("Open CASCADE STEP translator 7.6 10.1", 12, 0xc1f66a22ef60c905), ("Open CASCADE STEP translator 7.6 10.2", 8002, 0x8de3f0da571257bf)],
        "cone.step": [("Open CASCADE STEP translator 7.6 2", 4476, 0xa72428057d13715c)],
        "inch-box.step": [("Open CASCADE STEP translator 7.6 11", 12, 0x7195088887dc91a5)],
        "loft.step": [("Open CASCADE STEP translator 7.6 6", 13744, 0x271323a67d257f84)],
        "pocket.step": [("Open CASCADE STEP translator 7.6 8", 4267, 0xd6d34bd93bee85fb)],
        "rounded-box.step": [("Open CASCADE STEP translator 7.6 5", 524, 0xa4ba22e02835e90d)],
        "shell-and-face.step": [("Open CASCADE STEP translator 7.6 9.1", 10, 0x8a89bdfd61c43f15), ("Open CASCADE STEP translator 7.6 9.2", 2, 0x6eb9b51ac0f4f9ea)],
        "sphere.step": [("Open CASCADE STEP translator 7.6 1", 11922, 0x792d608423b80089)],
        "three-roots.step": [("Cone A", 3023, 0x07c28365f35e2204), ("Ring B", 31752, 0x7ac6b942eb0ede76), ("Open CASCADE STEP translator 7.6 4", 12, 0x508a1822718f23a5)],
        "torus.step": [("Open CASCADE STEP translator 7.6 3", 34524, 0x9aac6e5ef3869084)],
        "tube.step": [("Open CASCADE STEP translator 7.6 4", 1008, 0x1cee4ab4248e55b1)],
        "vase.step": [("Open CASCADE STEP translator 7.6 7", 36908, 0x1a8e0c6546d21b8e)],
    ]

    nonisolated static func files(in folder: String) throws -> [URL] {
        try FileManager.default.contentsOfDirectory(at: resources.appending(path: folder), includingPropertiesForKeys: nil)
            .filter { $0.pathExtension == "step" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
    }

    nonisolated static func fnv(_ part: Part) -> UInt64 {
        let m = part.mesh()
        let bytes = UnsafeRawBufferPointer(start: UnsafeRawPointer(bitPattern: UInt(m.address())), count: Int(m.lenBytes()))
        var h: UInt64 = 0xcbf2_9ce4_8422_2325
        for b in bytes {
            h ^= UInt64(b)
            h = h &* 0x100_0000_01b3
        }
        return h
    }

    @Test("every parity fixture comes through the service as load_parts made it in process")
    func parityFixtures() async throws {
        let files = try Self.files(in: "step-fixtures")
        #expect(files.count == Self.expected.count)
        var times: [String] = []
        for f in files {
            let start = ContinuousClock.now
            let parts = try await CoreLoader.parts(at: f.path(percentEncoded: false), stepTimeout: .seconds(20))
            times.append("\(f.lastPathComponent) \(ContinuousClock.now - start)")
            let got = parts.map { ($0.info().name, $0.info().triangles, Self.fnv($0)) }
            let want = try #require(Self.expected[f.lastPathComponent])
            #expect(got.map(\.0) == want.map(\.0), "\(f.lastPathComponent)")
            #expect(got.map(\.1) == want.map(\.1), "\(f.lastPathComponent)")
            #expect(got.map(\.2) == want.map(\.2), "\(f.lastPathComponent): the mesh")
        }
        print("StepReaderTests timings (service read + load_step_parts):", times.joined(separator: ", "))
    }

    /// The 104 mutants that crashed the app and the audit's two repros: each is an error, and this process lives on.
    /// 101 make OpenCascade fault (its `step-read` exits), 5 make it loop until the timeout ends the read (the service
    /// kills that `step-read`).
    @Test("every file that crashed the app is an error now")
    func filesThatCrashedTheApp() async throws {
        let files = try Self.files(in: "segv")
        #expect(files.count == 106)
        var reasons: [StepReader.Failure.Reason: Int] = [:]
        for f in files {
            do {
                let parts = try await CoreLoader.parts(at: f.path(percentEncoded: false), stepTimeout: .seconds(5))
                Issue.record("\(f.lastPathComponent): \(parts.count) parts")
            } catch let failure as StepReader.Failure {
                reasons[failure.reason, default: 0] += 1
            } catch {
                // OpenCascade read it after all and said why not: still an error, as the page shows it
                Issue.record("\(f.lastPathComponent): \(error)")
            }
        }
        print("StepReaderTests: \(reasons)")
        #expect(reasons[.crashed] == 101)
        #expect(reasons[.timeout] == 5)
        #expect(reasons[.connection] == nil)
        // and the service comes back for the next file
        let sphere = resources.appending(path: "step-fixtures/sphere.step").path(percentEncoded: false)
        #expect(try await CoreLoader.parts(at: sphere).count == 1)
    }

    @Test("the window says it could not read the file, prefix translated as for any file")
    func windowStatus() async throws {
        let url = resources.appending(path: "segv/step_segv_loft.step")
        let model = ViewerModel()
        await model.load(url: url)
        #expect(model.status == "Could not read step_segv_loft.step:\nCouldn't read this STEP file")
        #expect(!model.hasPart)
        let de = try #require(L10n.Language("de"))
        #expect(ReadFailure.text(of: StepReader.Failure(reason: .crashed), reading: "a.step", in: de)
            == "a.step konnte nicht gelesen werden:\nCouldn't read this STEP file")
        // the message follows the app's language (English here); in German:
        #expect(JS.t("Couldn't read this STEP file", in: de) == "Diese STEP-Datei konnte nicht gelesen werden")
    }

    @Test("a cancelled read ends at once, and the next read works")
    func cancel() async throws {
        // OpenCascade loops forever on this one
        let hang = resources.appending(path: "segv/assembly_015.step").path(percentEncoded: false)
        let task = Task { try await CoreLoader.parts(at: hang) }
        try await Task.sleep(for: .milliseconds(500))
        let start = ContinuousClock.now
        task.cancel()
        await #expect(throws: CancellationError.self) { _ = try await task.value }
        #expect(ContinuousClock.now - start < .seconds(2))
        let sphere = resources.appending(path: "step-fixtures/sphere.step").path(percentEncoded: false)
        #expect(try await CoreLoader.parts(at: sphere).count == 1)
    }

    @Test("a read that takes too long is an error")
    func timeout() async throws {
        let hang = resources.appending(path: "segv/sphere_013.step").path(percentEncoded: false)
        let start = ContinuousClock.now
        await #expect(throws: StepReader.Failure(reason: .timeout)) {
            _ = try await CoreLoader.parts(at: hang, stepTimeout: .seconds(2))
        }
        #expect(ContinuousClock.now - start < .seconds(10))
    }
}
