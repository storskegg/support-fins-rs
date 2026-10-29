import Foundation
import SupportFinsCore
import Testing

@testable import Support_Fins

/// What a user reads when a file fails (S-01): the web's `Could not read <name>:\n<message>` (`ui/io.js:407`) with the
/// engine's own message, never a Swift type dump.
@MainActor
@Suite("Readable errors")
struct ReadableErrorTests {
    nonisolated private static func file(_ name: String, _ bytes: Data) throws -> URL {
        let url = URL(fileURLWithPath: NSTemporaryDirectory() + name)
        try bytes.write(to: url)
        return url
    }

    /// A binary STL whose header promises 100 triangles and carries one.
    nonisolated private static func truncatedSTL() -> Data {
        var d = Data(count: 80)
        var n = UInt32(100)
        withUnsafeBytes(of: &n) { d.append(contentsOf: $0) }
        d.append(Data(count: 50))
        return d
    }

    struct Bad: Sendable {
        let name: String
        let bytes: Data
        let message: String
    }

    nonisolated static let shortSTL = "not an STL: too short for binary and no \"solid\" line for ASCII (truncated file?)"

    nonisolated static let cases: [Bad] = [
        Bad(name: "empty.stl", bytes: Data(), message: shortSTL),
        Bad(name: "truncated.stl", bytes: truncatedSTL(), message: shortSTL),
        Bad(name: "bad.3mf", bytes: Data("this is not a zip".utf8), message: "not a ZIP archive (no end-of-central-directory record)"),
    ]

    @Test("the window shows the web's text for a file that cannot be read", arguments: cases)
    func statusInEnglish(c: Bad) async throws {
        let model = ViewerModel()
        await model.load(url: try Self.file(c.name, c.bytes))
        #expect(model.status == "Could not read \(c.name):\n\(c.message)")
        #expect(!model.hasPart)
    }

    @Test("the prefix is translated, the engine's message stays as the web leaves it", arguments: cases)
    func statusInGerman(c: Bad) async throws {
        let url = try Self.file(c.name, c.bytes)
        let error = #expect(throws: FfiError.self) { _ = try loadParts(path: url.path(percentEncoded: false)) }
        let de = try #require(L10n.Language("de"))
        #expect(ReadFailure.text(of: try #require(error), reading: c.name, in: de) == "\(c.name) konnte nicht gelesen werden:\n\(c.message)")
    }

    @Test("an FfiError describes itself with the engine's message")
    func errorDescription() {
        #expect(FfiError.Format(message: "no triangles in the file").localizedDescription == "no triangles in the file")
        #expect(FfiError.Io(message: "disk full").localizedDescription == "disk full")
        #expect(FfiError.Engine(message: "nope").localizedDescription == "nope")
    }
}
